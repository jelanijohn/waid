// Auto-sync feeds: the pure half. Constants, per-brief tiers, per-kind
// rate-limit floors, per-credential budgets (token buckets), fingerprinting,
// and the interval/stretch/backoff math the scheduler
// (stores/autosync.svelte.ts) and the "new" bookkeeping
// (stores/integrations.svelte.ts) share. No Tauri, no DOM, no stores — same
// convention as lib/widget.ts. The settings tooltip text is generated from the
// same tables (`pollingRatesSummary`) so the UI and the cadence cannot drift.

import type { IntegrationItem, Provider } from "$lib/types";

/** Scheduler tick. */
export const TICK_MS = 5_000;
/** Feeds polled per tick. */
export const MAX_PER_TICK = 3;
/** Pause between two fetches in the same budget domain inside a tick. */
export const GAP_MS = 1_000;
/** Debounce for `kick()` (focus / online / visible). */
export const KICK_DEBOUNCE_MS = 5_000;
/** After a rate-limit error without a server-given wait, hold that domain this long. */
export const RATE_LIMIT_BACKOFF_MS = 60_000;
/** Longest wait after repeated failures. */
export const BACKOFF_CAP_MS = 3_600_000;
/** Longest a cached backend response may be reused by a background poll. */
export const POLL_MAX_AGE_CAP_MS = 60_000;
/** How stale a cached response a panel open (not a manual refresh) accepts. */
export const PANEL_MAX_AGE_MS = 60_000;

// --- Budgets -----------------------------------------------------------------
// Providers limit per *credential* (a GitHub user, a Slack workspace+app, a
// Gmail account…), so feeds are budgeted per domain = (class, credential id).
// The credential id is a session-salted hash Rust returns with every fetch;
// until a connection's first fetch its feeds share the provider-wide `class:*`.

export type BudgetClass =
  | "github-core"
  | "github-search"
  | "linear"
  | "jira"
  | "asana"
  | "notion"
  | "gmail"
  | "slack"
  | "figma";

/** Polled providers (NeuroSkill's `mind` feeds write a body region instead). */
export type PolledProvider = Exclude<Provider, "neuroskill">;

/** GitHub's search API has its own, much smaller bucket. */
export function budgetClass(p: PolledProvider, kind: string): BudgetClass {
  if (p === "github") return kind === "pulls" || kind === "commits" ? "github-search" : "github-core";
  return p;
}

/** Token bucket per class: sustained requests/min, burst size, and the minimum
 *  gap between two polls. Roughly half of each provider's published limit, since
 *  the same keys are usually shared with other tools. */
export const BUDGET: Record<BudgetClass, { perMin: number; burst: number; minGapMs: number }> = {
  "github-core": { perMin: 40, burst: 10, minGapMs: 1000 }, // ~50% of 83/min (5000/h)
  "github-search": { perMin: 12, burst: 4, minGapMs: 2000 }, // 50% of 30/min
  linear: { perMin: 12, burst: 4, minGapMs: 1000 },
  jira: { perMin: 20, burst: 5, minGapMs: 1000 },
  asana: { perMin: 60, burst: 10, minGapMs: 1000 },
  notion: { perMin: 60, burst: 6, minGapMs: 1000 },
  gmail: { perMin: 60, burst: 20, minGapMs: 1000 }, // cost per fetch ≈ 1 + limit
  slack: { perMin: 10, burst: 3, minGapMs: 3000 }, // Tier 2 incl. users.list pages
  figma: { perMin: 2, burst: 2, minGapMs: 15000 }, // 5/min on View seats
};

/** Requests a fetch is expected to make, reserved before it runs; the real
 *  count comes back as `IntegrationFetch.cost`. Gmail reads one message per
 *  item (default limit 15, max 50); a Notion database resolves its data source
 *  first (cached after the first fetch). */
export function estimatedCost(p: PolledProvider, kind: string, limit?: number | null): number {
  if (kind === "email") return 1 + Math.min(Math.max(limit ?? 15, 1), 50);
  if (p === "notion" && kind === "tasks") return 2;
  return 1;
}

/** `${class}:${credentialId ?? "*"}` */
export function domainKey(cls: BudgetClass, credentialId: string | null | undefined): string {
  return `${cls}:${credentialId ?? "*"}`;
}

/** The class half of a domain key. */
export function domainClass(domain: string): BudgetClass {
  return domain.slice(0, domain.lastIndexOf(":")) as BudgetClass;
}

export interface Bucket {
  tokens: number;
  at: number;
}

export function fullBucket(cls: BudgetClass, now: number): Bucket {
  return { tokens: BUDGET[cls].burst, at: now };
}

/** min(burst, tokens + perMin × elapsed minutes). Tokens may be negative after
 *  a fetch cost more than reserved; refill pays the debt down first. */
export function refill(b: Bucket, cls: BudgetClass, now: number): Bucket {
  const { perMin, burst } = BUDGET[cls];
  const elapsedMin = Math.max(0, now - b.at) / 60_000;
  return { tokens: Math.min(burst, b.tokens + perMin * elapsedMin), at: now };
}

/** How much slower than their tier the non-focused feeds of a domain must poll
 *  so the domain fits its budget. The focused brief's demand is served first;
 *  the rest share what's left, never less than 10% of capacity (so nothing
 *  starves). ≥ 1. */
export function stretchFor(demandPerMin: number, capacityPerMin: number, focusedDemandPerMin: number): number {
  if (demandPerMin <= capacityPerMin) return 1;
  const rest = demandPerMin - focusedDemandPerMin;
  if (rest <= 0) return 1;
  const room = Math.max(capacityPerMin - focusedDemandPerMin, capacityPerMin * 0.1);
  return Math.max(1, rest / room);
}

/** The focused brief's own stretch, when even it alone exceeds the budget. */
export function focusedStretchFor(focusedDemandPerMin: number, capacityPerMin: number): number {
  return Math.max(1, focusedDemandPerMin / capacityPerMin);
}

/** How long to hold a domain after an error, or null if it isn't a rate limit.
 *  Typed errors (IntegrationFetchError) carry the server's `Retry-After`;
 *  anything else falls back to matching the message. */
export function rateLimitHoldMs(e: unknown): number | null {
  if (e && typeof e === "object" && "kind" in e) {
    const { kind, retryAfterMs } = e as { kind?: unknown; retryAfterMs?: unknown };
    if (kind !== "rateLimited") return null;
    return typeof retryAfterMs === "number" && retryAfterMs > 0 ? retryAfterMs : RATE_LIMIT_BACKOFF_MS;
  }
  return /returned 429|rate limit/i.test(String(e)) ? RATE_LIMIT_BACKOFF_MS : null;
}

/** Hold until the provider's reset when it reports < 10% of a minute's budget
 *  left. Null when there's no reason to hold. */
export function lowRemainingHoldUntil(
  cls: BudgetClass,
  remaining: number | null | undefined,
  resetAt: string | null | undefined,
  now: number,
): number | null {
  if (remaining == null || remaining >= BUDGET[cls].perMin * 0.1 || !resetAt) return null;
  const t = Date.parse(resetAt);
  return Number.isFinite(t) && t > now ? Math.min(t, now + BACKOFF_CAP_MS) : null;
}

/** One budget domain as the UI sees it. */
export interface DomainStatus {
  domain: string;
  cls: BudgetClass;
  feeds: number;
  demandPerMin: number;
  capacityPerMin: number;
  /** Multiplier on non-focused intervals (1 = on schedule). */
  stretch: number;
  /** Multiplier on the focused brief's intervals (1 unless it alone overflows). */
  focusedStretch: number;
  /** Epoch ms the domain is paused until after a rate limit, or null. */
  holdUntil: number | null;
}

// --- Tiers -------------------------------------------------------------------

export type Tier = "focused" | "active" | "dormant" | "paused" | "archived";

/** Interval per brief tier. Archived briefs are never polled. */
export const TIER_MS: Record<Exclude<Tier, "archived">, number> = {
  focused: 20_000,
  active: 60_000,
  dormant: 1_800_000,
  paused: 600_000,
};

/** Briefs not opened for this many days poll on the dormant tier (Settings). */
export const DEFAULT_DORMANT_DAYS = 3;
export const DORMANT_DAYS_MIN = 1;
export const DORMANT_DAYS_MAX = 90;
export const DAY_MS = 86_400_000;

/** Per-kind floor from each provider's rate limits; applies in every tier.
 *  A server-reported `X-Poll-Interval` replaces it (see effectiveInterval). */
export const KIND_FLOOR_MS: Record<string, number> = {
  notifications: 60_000, // GitHub X-Poll-Interval minimum
  comments: 60_000, // Figma 5/min on View/Collab seats
  email: 30_000, // ~16 sequential requests per fetch
  messages: 30_000, // Slack Tier 2, incl. users.list paging
  tasks: 20_000,
  pulls: 20_000,
  commits: 20_000,
};

/** Never polled: `mind` writes a body region, `page` is a static doc. */
export const SKIP_KINDS = new Set(["mind", "page"]);
/** Fingerprint includes `updatedAt`; polled first within a tick. */
export const COMMS_KINDS = new Set(["notifications", "email", "messages", "comments"]);

/** Most fingerprints remembered per feed. */
export const SEEN_CAP = 300;

/** A brief's tier. Archived → never; the attention brief → focused; paused →
 *  paused; not opened within `dormantAfterMs` (or never) → dormant; else
 *  active. Unknown/custom statuses poll like `active` (status is a free
 *  string). Opening a brief stamps `lastOpened`, which promotes it. */
export function tierFor(
  status: string | null | undefined,
  isAttention: boolean,
  lastOpened?: string | null,
  dormantAfterMs: number = DEFAULT_DORMANT_DAYS * DAY_MS,
  now: number = Date.now(),
): Tier {
  const s = (status ?? "").trim().toLowerCase();
  if (s === "archived") return "archived";
  if (isAttention) return "focused";
  if (s === "paused") return "paused";
  const opened = lastOpened ? Date.parse(lastOpened) : NaN;
  if (!Number.isFinite(opened) || now - opened > dormantAfterMs) return "dormant";
  return "active";
}

/** max(tier, floor) × 2^min(failures, 5), capped. The floor is the server's
 *  requested interval when known, else the kind's. Infinity for archived. */
export function effectiveInterval(tier: Tier, kind: string, failures: number, serverFloorMs?: number | null): number {
  if (tier === "archived") return Infinity;
  const floor = serverFloorMs ?? KIND_FLOOR_MS[kind] ?? TIER_MS.focused;
  const base = Math.max(TIER_MS[tier], floor);
  return Math.min(base * 2 ** Math.min(failures, 5), BACKOFF_CAP_MS);
}

/** A ±10 % multiplier, drawn once per feed per cycle. */
export function jitter(): number {
  return 0.9 + Math.random() * 0.2;
}

/** cyrb53: a small, fast, non-cryptographic string hash (base-36). Collisions
 *  don't matter at this size; only hashes are ever stored. */
export function hash(str: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}

/** Hashed identity of an item for "new" detection. Comms kinds include
 *  `updatedAt` (every Figma comment shares the file URL; a notification thread
 *  keeps its URL as activity moves). Other kinds use the URL only, so the
 *  user's own edits to a task aren't flagged. `item.id` is a fallback only. */
export function fingerprint(kind: string, item: IntegrationItem): string {
  const base = item.url || item.id;
  return hash(COMMS_KINDS.has(kind) ? `${base}|${item.updatedAt ?? ""}` : base);
}

/** Gmail maps read mail to `read`; the user has already seen it. */
export function neverFresh(item: IntegrationItem): boolean {
  return item.status === "read";
}

function fmt(ms: number): string {
  if (ms < 120_000) return `${Math.round(ms / 1000)} s`;
  if (ms < 7_200_000) return `${Math.round(ms / 60_000)} min`;
  return `${Math.round(ms / 3_600_000)} h`;
}

// Tooltip wording for the kinds whose floor slows the focused tier.
const KIND_PHRASE: Record<string, string> = {
  email: "email",
  messages: "Slack",
  notifications: "GitHub notifications",
  comments: "Figma comments",
};

/** Human names for budget classes (tooltip + panel). */
export const CLASS_LABEL: Record<BudgetClass, string> = {
  "github-core": "GitHub",
  "github-search": "GitHub search",
  linear: "Linear",
  jira: "Jira",
  asana: "Asana",
  notion: "Notion",
  gmail: "Gmail",
  slack: "Slack",
  figma: "Figma",
};

/** "14:05" for a hold-until time. */
export function clockTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** "~4 min" — the approximate cadence of a stretched feed. */
export function approxEvery(ms: number): string {
  return `~${fmt(ms)}`;
}

/** The settings tooltip, built from TIER_MS + KIND_FLOOR_MS, plus a
 *  "Currently:" line for any live domain that's stretched or held. */
export function pollingRatesSummary(dormantDays: number, live: DomainStatus[] = [], now = Date.now()): string {
  const byFloor = new Map<number, string[]>();
  for (const [kind, floor] of Object.entries(KIND_FLOOR_MS)) {
    if (floor <= TIER_MS.focused || !KIND_PHRASE[kind]) continue;
    byFloor.set(floor, [...(byFloor.get(floor) ?? []), KIND_PHRASE[kind]]);
  }
  const slower = [...byFloor.entries()]
    .sort(([a], [b]) => a - b)
    .map(([floor, names]) => `${names.join(" & ")} ${fmt(floor)}`)
    .join(", ");
  const days = `${dormantDays} ${dormantDays === 1 ? "day" : "days"}`;
  const lines = [
    "Polling rates (while on)",
    `Brief you're viewing: every ${fmt(TIER_MS.focused)}${slower ? ` — ${slower}` : ""}`,
    `Active & blocked briefs: every ${fmt(TIER_MS.active)}`,
    `Paused briefs: every ${fmt(TIER_MS.paused)}`,
    `Dormant briefs (not opened in ${days}): every ${fmt(TIER_MS.dormant)}`,
    "Archived briefs: never",
    "Feeds sharing an API key share its rate limit; busy keys check less often.",
    "Slows down automatically after errors or rate limits.",
  ];

  // Merge domains of the same class (several tokens) into one phrase each.
  const notes = new Map<string, string>();
  for (const d of live) {
    const label = CLASS_LABEL[d.cls];
    if (d.holdUntil && d.holdUntil > now) {
      notes.set(label, `${label} paused until ${clockTime(d.holdUntil)}`);
    } else if (d.stretch > 1.2 && !notes.has(label)) {
      notes.set(label, `${label} ${d.stretch.toFixed(1)}× slower (${d.feeds} feeds share one key)`);
    }
  }
  if (notes.size) lines.push(`Currently: ${[...notes.values()].join("; ")}`);
  return lines.join("\n");
}
