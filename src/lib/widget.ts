// Pure helpers for widget mode: read a brief's body / the brief list and
// produce the short strings the widget shows. No Tauri, no DOM, no stores.

import type { Brief, BriefIntegration, IntegrationFetch } from "$lib/types";
import { STATUS_ORDER } from "$lib/status";

// Marker strings mirror SYNC_START / SYNC_END and marker_start/marker_end
// ("waid:state") in src-tauri/src/commands.rs.
const SYNC_START = "<!-- waid:sync:start -->";
const SYNC_END = "<!-- waid:sync:end -->";
const STATE_START = "<!-- waid:state:start -->";
const STATE_END = "<!-- waid:state:end -->";

/** NeuroSkill rolling windows (neuroskill::parse_mind_query). */
const MIND_WINDOWS = ["today", "7d", "14d", "30d"];

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

/** Plain-text first paragraph of the Current State (waid:state) region, or
 *  null. Markdown emphasis, code ticks, links and wikilinks reduce to their
 *  text; the result is shown via text interpolation, never as HTML. */
export function currentStateExcerpt(body: string): string | null {
  const block = between(body, STATE_START, STATE_END);
  if (block === null) return null;
  const text = block.trim().replace(/^##\s*Current State[^\n]*\n?/i, "");
  const para = text
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .find((p) => p.length > 0);
  if (!para) return null;
  const plain = para
    .replace(/\[\[([^\]|\n]+)\|([^\]\n]+)\]\]/g, "$2")
    .replace(/\[\[([^\]\n]+)\]\]/g, "$1")
    .replace(/\[([^\]\n]*)\]\([^)\n]*\)/g, "$1")
    .replace(/\*\*|__|`/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return plain || null;
}

/** One feed's count text for the leaf's live-state rows. */
export function feedCountText(
  ig: BriefIntegration,
  entry: { loading: boolean; error: string | null; data: IntegrationFetch | null } | null,
): string {
  if (ig.kind === "mind") {
    const tokens = (ig.query ?? "").trim().split(/\s+/).reverse();
    return tokens.find((t) => MIND_WINDOWS.includes(t.toLowerCase()))?.toLowerCase() ?? "14d";
  }
  if (ig.kind === "page") return "";
  if (entry?.data) {
    const { total, byStatus } = entry.data.summary;
    const statuses = Object.entries(byStatus);
    if (statuses.length === 1) return `${statuses[0][1]} ${statuses[0][0].toLowerCase()}`;
    return `${total} ${total === 1 ? "item" : "items"}`;
  }
  if (entry?.loading) return "…";
  if (entry?.error) return "—";
  return "";
}
