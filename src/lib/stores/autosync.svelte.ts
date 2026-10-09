// Auto-sync feeds: the background scheduler. While settings.autoSyncFeeds is
// on (wired by an effect in routes/+page.svelte), a timer re-runs the existing
// `fetch_integration` call through `integrations.poll` for every feed on every
// non-archived brief. Cadence is per brief tier (focused / active / paused) with
// per-kind rate-limit floors and per-provider gaps — all in lib/feedSync.ts.
//
// The poller only ever calls `integrations.poll`: never sync, synthesis,
// digest, briefing or capture, never a toast, never a write to a `.md`.

import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Provider } from "$lib/types";
import { projects } from "$lib/stores/projects.svelte";
import { widget } from "$lib/stores/widget.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { integrations, type FeedRef } from "$lib/stores/integrations.svelte";
import {
  TICK_MS,
  MAX_PER_TICK,
  GAP_MS,
  KICK_DEBOUNCE_MS,
  PROVIDER_GAP_MS,
  RATE_LIMIT_BACKOFF_MS,
  SKIP_KINDS,
  COMMS_KINDS,
  tierFor,
  effectiveInterval,
  jitter,
  isRateLimitError,
  type Tier,
} from "$lib/feedSync";

interface Feed extends FeedRef {
  key: string;
  limit?: number | null;
  provider: Provider;
  tier: Tier;
  attention: boolean;
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

class AutoSync {
  private timer: ReturnType<typeof setInterval> | null = null;
  private cleanups: (() => void)[] = [];
  private running = false;
  private lastKick = 0;

  // Scheduler state — in memory only.
  private lastAttempt = new Map<string, number>();
  private failures = new Map<string, number>();
  private jitters = new Map<string, number>();
  private lastProviderPoll = new Map<Provider, number>();
  private providerHold = new Map<Provider, number>();

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
    this.lastProviderPoll.clear();
    this.providerHold.clear();
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

  /** Every pollable feed on every brief (archived included, tagged), for both
   *  scheduling and `pruneSeen`. */
  private feeds(): Feed[] {
    const attention = this.attentionPath();
    const out: Feed[] = [];
    for (const b of projects.briefs) {
      const isAttention = b.path === attention;
      const tier = tierFor(b.status, isAttention);
      for (const ig of b.integrations) {
        if (SKIP_KINDS.has(ig.kind)) continue;
        const conn = b.connections.find((c) => c.id === ig.connection);
        if (!conn) continue;
        out.push({
          path: b.path,
          connection: ig.connection,
          kind: ig.kind,
          query: ig.query,
          limit: ig.limit,
          provider: conn.provider,
          tier,
          attention: isAttention,
          key: [b.path, ig.connection, ig.kind, ig.query ?? ""].join("\n"),
        });
      }
    }
    return out;
  }

  /** ms past due (≥ 0 means due), or null when never due (archived). */
  private overdue(f: Feed, now: number): number | null {
    if (f.tier === "archived") return null;
    const fetchedAt = integrations.get(f.path, f.connection, f.kind, f.query)?.data?.fetchedAt;
    const fetched = fetchedAt ? Date.parse(fetchedAt) : NaN;
    const last = Math.max(this.lastAttempt.get(f.key) ?? 0, Number.isFinite(fetched) ? fetched : 0);
    if (!last) return Infinity; // never fetched, never attempted
    let j = this.jitters.get(f.key);
    if (j === undefined) this.jitters.set(f.key, (j = jitter()));
    const interval = effectiveInterval(f.tier, f.kind, this.failures.get(f.key) ?? 0) * j;
    return now - last - interval;
  }

  private providerReady(p: Provider, now: number): boolean {
    if ((this.providerHold.get(p) ?? 0) > now) return false;
    return now - (this.lastProviderPoll.get(p) ?? 0) >= PROVIDER_GAP_MS[p];
  }

  private async tick(): Promise<void> {
    if (this.running || !this.timer) return;
    if (!navigator.onLine || document.visibilityState === "hidden") return;
    this.running = true;
    try {
      const all = this.feeds();
      // Skip while the list is (re)loading so a transient empty list can't
      // wipe every feed's seen state.
      if (!projects.loading && projects.briefs.length) integrations.pruneSeen(all);

      const now = Date.now();
      const due = all
        .map((f) => ({ f, by: this.overdue(f, now) }))
        .filter((d): d is { f: Feed; by: number } => d.by !== null && d.by >= 0)
        .sort(
          (a, b) =>
            Number(b.f.attention) - Number(a.f.attention) ||
            Number(COMMS_KINDS.has(b.f.kind)) - Number(COMMS_KINDS.has(a.f.kind)) ||
            b.by - a.by,
        );

      let polled = 0;
      for (const { f } of due) {
        if (polled >= MAX_PER_TICK || !this.timer) break;
        const at = Date.now();
        if (!this.providerReady(f.provider, at)) continue;
        if (polled > 0) await sleep(GAP_MS);
        this.lastProviderPoll.set(f.provider, at);
        this.lastAttempt.set(f.key, at);
        this.jitters.delete(f.key); // fresh jitter for the next cycle
        polled++;
        const res = await integrations.poll(f.path, f.connection, f.kind, f.query, f.limit);
        if (!res) continue; // a foreground fetch was already in flight
        if (res.ok) {
          this.failures.delete(f.key);
        } else {
          this.failures.set(f.key, (this.failures.get(f.key) ?? 0) + 1);
          if (isRateLimitError(res.error)) this.providerHold.set(f.provider, Date.now() + RATE_LIMIT_BACKOFF_MS);
        }
      }
    } finally {
      this.running = false;
    }
  }
}

export const autosync = new AutoSync();
