// Front-end-only metadata for the PM-integration providers. Brand colors,
// monograms, blurbs, and the per-provider form requirements all live here so
// the integrations modal, the in-brief panel, and ProviderTile stay in sync.
// (The Rust backend owns the actual fetch/validate; this is purely display +
// which form fields a provider needs.)

import type { Provider } from "./types";

export type Kind = "tasks" | "notifications" | "page" | "email" | "messages";

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
  notion: {
    label: "Notion",
    mono: "N",
    blurb: "A database's rows or a page",
    queryPlaceholder: "database id or URL",
    kinds: ["tasks", "page"],
    needsBaseUrl: false,
    needsAccount: false,
  },
  gmail: {
    label: "Gmail",
    // "G" is taken by GitHub; "@" reads as email and avoids the clash.
    mono: "@",
    blurb: "Recent emails matching a search",
    queryPlaceholder: "from:acme.com newer_than:14d",
    kinds: ["email"],
    needsBaseUrl: false,
    // The account is discovered by the OAuth flow, not typed (see IntegrationsModal).
    needsAccount: true,
  },
  slack: {
    label: "Slack",
    mono: "S",
    blurb: "Recent messages matching a search",
    queryPlaceholder: "in:#waid after:2026-06-01",
    kinds: ["messages"],
    needsBaseUrl: false,
    needsAccount: false,
  },
};

export const PROVIDER_ORDER: Provider[] = ["linear", "jira", "asana", "github", "notion", "gmail", "slack"];

/** Material glyph for a kind (used in segmented controls + feed rows). For Notion
 *  the same `kind` values mean "database" vs "page", so they render differently. */
export function kindIcon(kind: string, provider?: Provider): string {
  if (kind === "email") return "mail";
  if (kind === "messages") return "chat";
  if (provider === "notion") return kind === "page" ? "description" : "table";
  return kind === "notifications" ? "notifications" : "checklist";
}

/** Human label for a kind. Notion reuses `tasks`/`page` to mean database vs page. */
export function kindLabel(kind: string, provider?: Provider): string {
  if (kind === "email") return "Email";
  if (kind === "messages") return "Messages";
  if (provider === "notion") return kind === "page" ? "Project page" : "Database / table";
  return kind === "notifications" ? "Notifications" : "Tasks";
}
