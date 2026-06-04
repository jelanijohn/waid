// Front-end-only metadata for the PM-integration providers. Brand colors,
// monograms, blurbs, and the per-provider form requirements all live here so
// the integrations modal, the in-brief panel, and ProviderTile stay in sync.
// (The Rust backend owns the actual fetch/validate; this is purely display +
// which form fields a provider needs.)

import type { Provider } from "./types";

export type Kind = "tasks" | "notifications";

export interface ProviderMeta {
  label: string;
  /** Monogram letter shown in the tile. */
  mono: string;
  /** One-line description used on the pick cards + empty hints. */
  blurb: string;
  /** Mono placeholder for the "Filter (optional)" / query input. */
  queryPlaceholder: string;
  /** Kinds this provider can pull. Only GitHub exposes notifications today. */
  kinds: Kind[];
  /** Jira cloud instance / GitHub Enterprise base URL. */
  needsBaseUrl: boolean;
  /** Jira account email / Asana workspace id. */
  needsAccount: boolean;
}

export const PROVIDERS: Record<Provider, ProviderMeta> = {
  linear: {
    label: "Linear",
    mono: "L",
    blurb: "Issues assigned to you",
    queryPlaceholder: "assignee:me state:started",
    kinds: ["tasks"],
    needsBaseUrl: false,
    needsAccount: false,
  },
  jira: {
    label: "Jira",
    mono: "J",
    blurb: "Issues from a JQL filter",
    queryPlaceholder: "project = WAID AND statusCategory != Done",
    kinds: ["tasks"],
    needsBaseUrl: true,
    needsAccount: true,
  },
  asana: {
    label: "Asana",
    mono: "A",
    blurb: "Tasks from a project",
    queryPlaceholder: "my_tasks",
    kinds: ["tasks"],
    needsBaseUrl: false,
    needsAccount: true,
  },
  github: {
    label: "GitHub",
    mono: "G",
    blurb: "Issues, PRs & notifications",
    queryPlaceholder: "is:open assignee:@me",
    kinds: ["tasks", "notifications"],
    needsBaseUrl: true,
    needsAccount: false,
  },
};

export const PROVIDER_ORDER: Provider[] = ["linear", "jira", "asana", "github"];

/** Material glyph for a kind (used in segmented controls + feed rows). */
export function kindIcon(kind: string): string {
  return kind === "notifications" ? "notifications" : "checklist";
}

/** Human label for a kind. */
export function kindLabel(kind: string): string {
  return kind === "notifications" ? "Notifications" : "Tasks";
}
