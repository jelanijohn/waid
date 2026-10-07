// Pure helpers for widget mode: read a brief's body / the brief list and
// produce the short strings the widget shows. No Tauri, no DOM, no stores.

import type { Brief } from "$lib/types";
import { STATUS_ORDER } from "$lib/status";

// Marker strings mirror SYNC_START / SYNC_END in src-tauri/src/commands.rs
// (and marker_start("waid:sync"), which equals SYNC_START by construction).
const SYNC_START = "<!-- waid:sync:start -->";
const SYNC_END = "<!-- waid:sync:end -->";

/** Text between a start and end marker, or null if either is missing or the
 *  end comes first. */
function between(body: string, start: string, end: string): string | null {
  const s = body.indexOf(start);
  const e = body.indexOf(end);
  if (s === -1 || e === -1 || e < s) return null;
  return body.slice(s + start.length, e);
}

/** Detail of the first OK source line in the managed sync (## Activity)
 *  block, minus the `last push …` segment (relative to sync time, so it goes
 *  stale), or null. Lines look like `**{label}** · {detail}`; labels can
 *  contain ` · ` themselves (`GitHub · owner/repo`), so split on the closing
 *  `**`, not the first separator. Error lines (`⚠️ …`) are skipped. */
export function activityLine(body: string): string | null {
  const block = between(body, SYNC_START, SYNC_END);
  if (block === null) return null;
  for (const raw of block.split("\n")) {
    const line = raw.trim();
    if (!line.startsWith("**")) continue;
    const close = line.indexOf("**", 2);
    if (close === -1) continue;
    const rest = line.slice(close + 2);
    if (!rest.startsWith(" · ")) continue;
    const detail = rest.slice(3).trim();
    if (detail.startsWith("⚠️")) continue;
    const kept = detail
      .split(" · ")
      .map((s) => s.trim())
      .filter((s) => s && !s.startsWith("last push "));
    const out = kept.join(" · ");
    if (out) return out;
  }
  return null;
}

/** Counts per known status over non-archived briefs, in STATUS_ORDER, zero
 *  counts dropped. Custom statuses aren't counted. */
export function statusTally(briefs: Brief[]): { status: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const b of briefs) {
    const s = (b.status ?? "").toLowerCase();
    if (s) counts.set(s, (counts.get(s) ?? 0) + 1);
  }
  return STATUS_ORDER.filter((s) => s !== "archived")
    .map((status) => ({ status, count: counts.get(status) ?? 0 }))
    .filter((t) => t.count > 0);
}
