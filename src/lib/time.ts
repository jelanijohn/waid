/** Format an ISO timestamp as a short relative string, e.g. "3h ago". */
export function relativeTime(iso?: string | null): string {
  if (!iso) return "never opened";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";

  const seconds = Math.round((Date.now() - then) / 1000);
  if (seconds < 60) return "just now";

  const units: [number, string][] = [
    [60, "min"],
    [60, "hr"],
    [24, "day"],
    [7, "week"],
    [4.345, "month"],
    [12, "year"],
  ];

  let value = seconds;
  let unit = "sec";
  for (const [factor, name] of units) {
    if (value < factor) break;
    value = Math.floor(value / factor);
    unit = name;
  }
  const plural = value === 1 ? "" : "s";
  return `${value} ${unit}${plural} ago`;
}

/** Compact relative time for tight rows: "now", "5m", "11h", "6d", "1w",
 *  "3mo", "2y". Same unit ladder as relativeTime; "—" for null/unparseable. */
export function shortAgo(iso?: string | null): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";

  const seconds = Math.round((Date.now() - then) / 1000);
  if (seconds < 60) return "now";

  const units: [number, string][] = [
    [60, "m"],
    [60, "h"],
    [24, "d"],
    [7, "w"],
    [4.345, "mo"],
    [12, "y"],
  ];

  let value = seconds;
  let unit = "s";
  for (const [factor, name] of units) {
    if (value < factor) break;
    value = Math.floor(value / factor);
    unit = name;
  }
  return `${value}${unit}`;
}
