// Auto-sync feeds: the background scheduler. While settings.autoSyncFeeds is
// on (wired by an effect in routes/+page.svelte), a timer re-runs the existing
// `fetch_integration` call through `integrations.poll` for every feed on every
// non-archived brief. Cadence is per brief tier (focused / active / dormant /
// paused) with per-kind rate-limit floors; the pacing unit is the budget
// domain (budget class × credential), each with a token bucket debited by the
// requests the backend reports. When a domain's demand exceeds its budget the
// non-focused feeds in it are stretched (never starved), and `domains` tells
// the UI by how much. All the math is in lib/feedSync.ts.
//
// The poller only ever calls `integrations.poll`: never sync, synthesis,
// digest, briefing or capture, never a toast, never a write to a `.md`.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { projects } from "$lib/stores/projects.svelte";
import { widget } from "$lib/stores/widget.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { integrations, type FeedRef } from "$lib/stores/integrations.svelte";
import {
  TICK_MS,
  MAX_PER_TICK,
  GAP_MS,
  KICK_DEBOUNCE_MS,
  POLL_MAX_AGE_CAP_MS,
  DAY_MS,
  BUDGET,
  SKIP_KINDS,
  COMMS_KINDS,
  budgetClass,
  domainKey,
  domainClass,
  estimatedCost,
  fullBucket,
  refill,
  stretchFor,
  focusedStretchFor,
  rateLimitHoldMs,
  lowRemainingHoldUntil,
  tierFor,
  effectiveInterval,
  jitter,
  type Bucket,
  type BudgetClass,
  type DomainStatus,
  type PolledProvider,
  type Tier,
} from "$lib/feedSync";

interface Feed extends FeedRef {
  key: string;
  limit?: number | null;
  provider: PolledProvider;
  tier: Tier;
  attention: boolean;
  cls: BudgetClass;
  domain: string;
  estCost: number;
  /** False when the selector says `poll: false` (manual refresh only). */
  background: boolean;
}

/** What the panel needs to describe one feed's cadence. */
interface FeedInfo {
  domain: string;
  intervalMs: number;
  attention: boolean;
}

/** A feed's effective cadence, when auto-sync is scheduling it. */
export interface FeedCadence {
  /** Effective interval after any budget stretch. */
  everyMs: number;
  /** Polled noticeably (>1.2×) slower than its tier because its key is busy. */
  stretched: boolean;
  /** Paused by a rate limit until this epoch ms, or null. */
  holdUntil: number | null;
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Identity of a feed across ticks (and for `cadenceFor`). */
function feedKeyOf(path: string, connection: string, kind: string, query?: string | null): string {
  return [path, connection, kind, query ?? ""].join("\n");
}

class AutoSync {
  private timer: ReturnType<typeof setInterval> | null = null;
  private cleanups: (() => void)[] = [];
  private running = false;
  private lastKick = 0;

  // Scheduler state — in memory only. Per feed:
  private lastAttempt = new Map<string, number>();
  private failures = new Map<string, number>();
  private jitters = new Map<string, number>();
  /** Server-requested minimum interval (X-Poll-Interval), per feed. */
  private serverFloor = new Map<string, number>();
  // Per budget domain:
  private buckets = new Map<string, Bucket>();
  private holds = new Map<string, number>();
  private lastDomainPoll = new Map<string, number>();

  /** Live budget state per domain, for the settings tooltip and the panel. */
  domains = $state<Record<string, DomainStatus>>({});
  /** Per-feed cadence inputs, republished only when they change. */
  private feedInfo = $state.raw<Record<string, FeedInfo>>({});

  /** The settings toggle: flips the setting, then wipes (and, when turning
   *  on, re-baselines) the seen state. */
  setEnabled(on: boolean): void {
    settings.setAutoSyncFeeds(on);
    integrations.resetSeen();
  }

  /** Idempotent: timer + listeners, then an immediate kick. */
  start(): void {
    if (this.timer) return;
    this.timer = setInterval(() => void this.tick(), TICK_MS);

    const onFocus = () => this.kick();
    const onVisible = () => {
      if (document.visibilityState === "visible") this.kick();
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("online", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    this.cleanups.push(() => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("online", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    });

    // Tauri focus as well (WidgetShell listens to both for the same reason).
    let disposed = false;
    (async () => {
      try {
        const un = await getCurrentWindow().onFocusChanged(({ payload }) => {
          if (payload) this.kick();
        });
        if (disposed) un();
        else this.cleanups.push(un);
      } catch {
        // Window APIs unavailable (plain `vite dev`): DOM events still work.
      }
    })();
    this.cleanups.push(() => (disposed = true));

    this.kick();
  }

  /** Clears the timer and every listener. Scheduler state is dropped too. */
  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    this.cleanups.forEach((fn) => fn());
    this.cleanups = [];
    this.lastAttempt.clear();
    this.failures.clear();
    this.jitters.clear();
    this.serverFloor.clear();
    this.buckets.clear();
    this.holds.clear();
    this.lastDomainPoll.clear();
    this.domains = {};
    this.feedInfo = {};
  }

  /** How often a feed is actually being checked, or null when auto-sync isn't
   *  scheduling it (off, archived, `poll: false`). Reactive: reads `domains`. */
  cadenceFor(path: string, connection: string, kind: string, query?: string | null): FeedCadence | null {
    const info = this.feedInfo[feedKeyOf(path, connection, kind, query)];
    const d = info && this.domains[info.domain];
    if (!info || !d) return null;
    const mult = info.attention ? d.focusedStretch : d.stretch;
    return {
      everyMs: info.intervalMs * mult,
      stretched: mult > 1.2,
      holdUntil: d.holdUntil && d.holdUntil > Date.now() ? d.holdUntil : null,
    };
  }

  /** "The user is looking": run a tick now (debounced). The attention brief is
   *  on the fastest tier, so its feeds are the first due. */
  kick(): void {
    if (!this.timer) return;
    const now = Date.now();
    if (now - this.lastKick < KICK_DEBOUNCE_MS) return;
    this.lastKick = now;
    void this.tick();
  }

  /** The brief in front of the user: the dashboard selection, or in widget
   *  mode the open leaf, else the lead row. */
  private attentionPath(): string | null {
    if (widget.mode === "widget") return (widget.openBrief ?? widget.lead)?.path ?? null;
    return projects.selectedPath;
  }

  /** Every pollable feed on every brief (archived and `poll: false` included,
   *  tagged), for both scheduling and `pruneSeen`. */
  private feeds(now: number): Feed[] {
    const attention = this.attentionPath();
    const dormantAfterMs = settings.dormantAfterDays * DAY_MS;
    const out: Feed[] = [];
    for (const b of projects.briefs) {
      const isAttention = b.path === attention;
      const tier = tierFor(b.status, isAttention, b.lastOpened, dormantAfterMs, now);
      for (const ig of b.integrations) {
        if (SKIP_KINDS.has(ig.kind)) continue;
        const conn = b.connections.find((c) => c.id === ig.connection);
        if (!conn || conn.provider === "neuroskill") continue;
        const cls = budgetClass(conn.provider, ig.kind);
        out.push({
          path: b.path,
          connection: ig.connection,
          kind: ig.kind,
          query: ig.query,
          limit: ig.limit,
          provider: conn.provider,
          tier,
          attention: isAttention,
          cls,
          domain: domainKey(cls, integrations.credentialIdFor(b.path, ig.connection)),
          estCost: estimatedCost(conn.provider, ig.kind, ig.limit),
          background: ig.poll !== false,
          key: feedKeyOf(b.path, ig.connection, ig.kind, ig.query),
        });
      }
    }
    return out;
  }

  /** The feed's tier interval with backoff and any server floor (no jitter, no stretch). */
  private interval(f: Feed): number {
    return effectiveInterval(f.tier, f.kind, this.failures.get(f.key) ?? 0, this.serverFloor.get(f.key));
  }

  /** ms past due (≥ 0 means due) with the domain's stretch applied, or null
   *  when never due (archived). */
  private overdue(f: Feed, now: number, stretch: number): number | null {
    if (f.tier === "archived") return null;
    const fetchedAt = integrations.get(f.path, f.connection, f.kind, f.query)?.data?.fetchedAt;
    const fetched = fetchedAt ? Date.parse(fetchedAt) : NaN;
    const last = Math.max(this.lastAttempt.get(f.key) ?? 0, Number.isFinite(fetched) ? fetched : 0);
    if (!last) return Infinity; // never fetched, never attempted
    let j = this.jitters.get(f.key);
    if (j === undefined) this.jitters.set(f.key, (j = jitter()));
    return now - last - this.interval(f) * j * stretch;
  }

  private bucket(domain: string, now: number): Bucket {
    let b = this.buckets.get(domain);
    if (!b) this.buckets.set(domain, (b = fullBucket(domainClass(domain), now)));
    return b;
  }

  /** Refill every bucket, then debit what was spent since the last settle —
   *  background polls and foreground fetches alike. */
  private settle(now: number): void {
    for (const [d, b] of this.buckets) this.buckets.set(d, refill(b, domainClass(d), now));
    for (const [d, cost] of integrations.takeSpend()) this.bucket(d, now).tokens -= cost;
  }

  /** Per-domain demand vs budget → stretch, published for the UI. */
  private assess(polled: Feed[], now: number): Record<string, DomainStatus> {
    const acc = new Map<string, { cls: BudgetClass; feeds: number; demand: number; focused: number }>();
    for (const f of polled) {
      const a = acc.get(f.domain) ?? { cls: f.cls, feeds: 0, demand: 0, focused: 0 };
      const perMin = f.estCost / (this.interval(f) / 60_000);
      a.feeds++;
      a.demand += perMin;
      if (f.attention) a.focused += perMin;
      acc.set(f.domain, a);
    }
    const out: Record<string, DomainStatus> = {};
    for (const [domain, a] of acc) {
      const capacity = BUDGET[a.cls].perMin;
      const hold = this.holds.get(domain) ?? 0;
      if (hold && hold <= now) this.holds.delete(domain);
      out[domain] = {
        domain,
        cls: a.cls,
        feeds: a.feeds,
        demandPerMin: Math.round(a.demand * 100) / 100,
        capacityPerMin: capacity,
        stretch: Math.round(stretchFor(a.demand, capacity, a.focused) * 100) / 100,
        focusedStretch: Math.round(focusedStretchFor(a.focused, capacity) * 100) / 100,
        holdUntil: hold > now ? hold : null,
      };
    }
    return out;
  }

  /** Republish `domains` / `feedInfo` only when they changed, so the tooltip
   *  and panel don't re-render every tick. */
  private publish(status: Record<string, DomainStatus>, polled: Feed[]): void {
    if (JSON.stringify(status) !== JSON.stringify(this.domains)) this.domains = status;
    const info: Record<string, FeedInfo> = {};
    for (const f of polled) info[f.key] = { domain: f.domain, intervalMs: this.interval(f), attention: f.attention };
    if (JSON.stringify(info) !== JSON.stringify(this.feedInfo)) this.feedInfo = info;
  }

  private async tick(): Promise<void> {
    if (this.running || !this.timer) return;
    if (!navigator.onLine || document.visibilityState === "hidden") return;
    this.running = true;
    try {
      const now = Date.now();
      const all = this.feeds(now);
      // Skip while the list is (re)loading so a transient empty list can't
      // wipe every feed's seen state.
      if (!projects.loading && projects.briefs.length) integrations.pruneSeen(all);

      this.settle(now);
      const polled = all.filter((f) => f.background && f.tier !== "archived");
      const status = this.assess(polled, now);
      this.publish(status, polled);

      // Due feeds, queued per domain: attention first, comms first, then most
      // overdue relative to their interval.
      const queues = new Map<string, { f: Feed; ratio: number }[]>();
      for (const f of polled) {
        const d = status[f.domain];
        const by = this.overdue(f, now, f.attention ? d.focusedStretch : d.stretch);
        if (by === null || by < 0) continue;
        const q = queues.get(f.domain) ?? [];
        q.push({ f, ratio: by / this.interval(f) });
        queues.set(f.domain, q);
      }
      const rank = (a: { f: Feed; ratio: number }, b: { f: Feed; ratio: number }) =>
        Number(b.f.attention) - Number(a.f.attention) ||
        Number(COMMS_KINDS.has(b.f.kind)) - Number(COMMS_KINDS.has(a.f.kind)) ||
        b.ratio - a.ratio;
      for (const q of queues.values()) q.sort(rank);

      // Round-robin across domains, the one holding the attention brief first,
      // then the most starved. One domain can't use up the tick.
      const order = [...queues.keys()].sort((a, b) => rank(queues.get(a)![0], queues.get(b)![0]));
      const picks: Feed[] = [];
      let progressed = true;
      while (picks.length < MAX_PER_TICK && progressed) {
        progressed = false;
        for (const d of order) {
          if (picks.length >= MAX_PER_TICK) break;
          const head = queues.get(d)![0];
          if (!head) continue;
          const f = head.f;
          const { burst, minGapMs } = BUDGET[f.cls];
          if ((this.holds.get(d) ?? 0) > now) continue;
          if (now - (this.lastDomainPoll.get(d) ?? 0) < minGapMs) continue;
          const b = this.bucket(d, now);
          // A fetch costlier than the whole burst (a big Gmail limit) runs on a
          // full bucket and leaves it in debt.
          if (b.tokens < Math.min(f.estCost, burst)) continue;
          b.tokens -= f.estCost;
          this.lastDomainPoll.set(d, now);
          picks.push(f);
          queues.get(d)!.shift();
          progressed = true;
        }
      }

      let prev: string | null = null;
      for (const f of picks) {
        if (!this.timer) break;
        if (prev === f.domain) await sleep(GAP_MS);
        prev = f.domain;
        // Selection reserved the domain; restamp at dispatch so `minGapMs` runs
        // from the real request, not from a pick made before slower fetches.
        this.lastDomainPoll.set(f.domain, Date.now());
        this.lastAttempt.set(f.key, Date.now());
        this.jitters.delete(f.key); // fresh jitter for the next cycle
        const maxAge = Math.min(this.interval(f) / 2, POLL_MAX_AGE_CAP_MS);
        const res = await integrations.poll(f.path, f.connection, f.kind, f.query, f.limit, maxAge);
        // Return the reservation; the ledger carries what was really spent
        // (0 for a cache hit or a 304), debited at the next settle.
        this.bucket(f.domain, Date.now()).tokens += f.estCost;
        if (!res) continue; // a foreground fetch was already in flight
        // The first fetch may have revealed the credential: account to it.
        const domain = domainKey(f.cls, integrations.credentialIdFor(f.path, f.connection));
        if (res.ok) {
          this.failures.delete(f.key);
          const rate = res.data.rate;
          if (rate?.minIntervalMs) this.serverFloor.set(f.key, rate.minIntervalMs);
          else this.serverFloor.delete(f.key);
          const until = lowRemainingHoldUntil(f.cls, rate?.remaining, rate?.resetAt, Date.now());
          if (until) this.holds.set(domain, until);
        } else {
          this.failures.set(f.key, (this.failures.get(f.key) ?? 0) + 1);
          const hold = rateLimitHoldMs(res.cause);
          if (hold !== null) this.holds.set(domain, Date.now() + hold);
        }
      }
    } finally {
      this.running = false;
    }
  }
}

export const autosync = new AutoSync();
