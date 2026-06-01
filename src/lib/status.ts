// Status / "rock priority" palette shared by the sidebar, detail pane, and
// status pill. Colors are CSS variables (defined in app.css) so they stay in
// sync with the theme; elements set `--sc` to one of these to derive tints.

export const STATUS_ORDER = ["active", "paused", "blocked", "archived"] as const;

export const STATUS_COLOR: Record<string, string> = {
  active: "var(--status-active)",
  paused: "var(--status-paused)",
  blocked: "var(--status-blocked)",
  archived: "var(--status-archived)",
};

export const STATUS_LABEL: Record<string, string> = {
  active: "Active",
  paused: "Paused",
  blocked: "Blocked",
  archived: "Archived",
};

/** CSS color for a status, falling back to the neutral "archived" grey. */
export function statusColor(status?: string | null): string {
  return STATUS_COLOR[(status ?? "").toLowerCase()] ?? "var(--status-archived)";
}
