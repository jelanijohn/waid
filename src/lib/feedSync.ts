// Auto-sync feeds: the pure half. Constants, per-brief tiers, per-kind
// rate-limit floors, fingerprinting, and the interval/backoff math the
// scheduler (stores/autosync.svelte.ts) and the "new" bookkeeping
// (stores/integrations.svelte.ts) share. No Tauri, no DOM, no stores — same
// convention as lib/widget.ts. The settings tooltip text is generated from the
// same tables (`pollingRatesSummary`) so the UI and the cadence cannot drift.

import type { IntegrationItem, Provider } from "$lib/types";

/** Scheduler tick. */
export const TICK_MS = 5_000;
/** Feeds polled per tick. */
export const MAX_PER_TICK = 3;
/** Pause between fetches inside a tick. */
export const GAP_MS = 1_000;
/** Debounce for `kick()` (focus / online / visible). */
export const KICK_DEBOUNCE_MS = 5_000;
/** Minimum time between two polls of the same provider, ticks and kicks alike. */
export const PROVIDER_GAP_MS: Record<Provider, number> = {
  github: 5_000,
  linear: 5_000,
  jira: 5_000,
  asana: 5_000,
  notion: 5_000,
  gmail: 10_000,
  slack: 15_000,
  figma: 15_000,
  neuroskill: 60_000, // never polled (`mind` is in SKIP_KINDS); here for completeness
};
/** After a rate-limit error, hold that provider at least this long. */
export const RATE_LIMIT_BACKOFF_MS = 60_000;
/** Longest wait after repeated failures. */
export const BACKOFF_CAP_MS = 3_600_000;

export type Tier = "focused" | "active" | "paused" | "archived";

/** Interval per brief tier. Archived briefs are never polled. */
export const TIER_MS: Record<Exclude<Tier, "archived">, number> = {
  focused: 20_000,
  active: 60_000,
  paused: 600_000,
};

/** Per-kind floor from each provider's rate limits; applies in every tier. */
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

/** A brief's tier from its status and whether it's the attention brief.
 *  Unknown/custom statuses poll like `active` (status is a free string). */
export function tierFor(status: string | null | undefined, isAttention: boolean): Tier {
  const s = (status ?? "").trim().toLowerCase();
  if (s === "archived") return "archived";
  if (isAttention) return "focused";
  if (s === "paused") return "paused";
  return "active";
}

/** max(tier, kind floor) × 2^min(failures, 5), capped. Infinity for archived. */
export function effectiveInterval(tier: Tier, kind: string, failures: number): number {
  if (tier === "archived") return Infinity;
  const base = Math.max(TIER_MS[tier], KIND_FLOOR_MS[kind] ?? TIER_MS.focused);
  return Math.min(base * 2 ** Math.min(failures, 5), BACKOFF_CAP_MS);
}

/** A ±10 % multiplier, drawn once per feed per cycle. */
export function jitter(): number {
  return 0.9 + Math.random() * 0.2;
}

/** Slack's in-band "rate limit hit" string, or `read_json`'s "returned 429". */
export function isRateLimitError(error: string): boolean {
  return /returned 429|rate limit/i.test(error);
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
  return ms < 120_000 ? `${Math.round(ms / 1000)} s` : `${Math.round(ms / 60_000)} min`;
}

// Tooltip wording for the kinds whose floor slows the focused tier.
const KIND_PHRASE: Record<string, string> = {
  email: "email",
  messages: "Slack",
  notifications: "GitHub notifications",
  comments: "Figma comments",
};

/** The settings tooltip, built from TIER_MS + KIND_FLOOR_MS. */
export function pollingRatesSummary(): string {
  const byFloor = new Map<number, string[]>();
  for (const [kind, floor] of Object.entries(KIND_FLOOR_MS)) {
    if (floor <= TIER_MS.focused || !KIND_PHRASE[kind]) continue;
    byFloor.set(floor, [...(byFloor.get(floor) ?? []), KIND_PHRASE[kind]]);
  }
  const slower = [...byFloor.entries()]
    .sort(([a], [b]) => a - b)
    .map(([floor, names]) => `${names.join(" & ")} ${fmt(floor)}`)
    .join(", ");
  return [
    "Polling rates (while on)",
    `Brief you're viewing: every ${fmt(TIER_MS.focused)}${slower ? ` — ${slower}` : ""}`,
    `Active & blocked briefs: every ${fmt(TIER_MS.active)}`,
    `Paused briefs: every ${fmt(TIER_MS.paused)}`,
    "Archived briefs: never",
    "Slows down automatically after errors or rate limits.",
  ].join("\n");
}
