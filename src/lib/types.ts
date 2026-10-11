// Mirrors the structs returned by the Rust backend (see src-tauri/src/commands.rs).
// Field names are camelCase because the Rust structs use `#[serde(rename_all = "camelCase")]`.

export interface Link {
  label: string;
  url: string;
}

export interface WebhookHeader {
  name: string;
  /** May contain the literal `{{secret}}`, resolved from the keyring at fire time. */
  value: string;
}

export interface Webhook {
  /** Stable slug, e.g. "deploy-staging". Scopes the keyring secret; survives
   *  relabeling. Synthesized from the label on first save when absent. */
  id: string;
  label: string;
  url: string;
  method: string;
  body?: string | null;
  /** Custom request headers; one value may reference the keyring secret via `{{secret}}`. */
  headers?: WebhookHeader[];
}

/** A sync source for pulling external data into a brief (see sync_brief).
 *  GitHub repos are derived from `links`; this covers everything else. */
export interface SyncSource {
  label: string;
  url: string;
  method?: string | null;
  /** Flat `json_path -> label` extraction map. */
  fields: Record<string, string>;
}

/** Per-brief result of a "Sync all" run (see sync_all). */
export interface SyncOutcome {
  path: string;
  name: string;
  ok: boolean;
  error?: string | null;
}

/** A supported project-management provider (see src-tauri/src/provider). */
export type Provider = "linear" | "jira" | "asana" | "github" | "notion" | "gmail" | "slack" | "figma" | "neuroskill";

/** An account-level connection to a provider. Metadata only — the API token
 *  lives in the OS keyring keyed by `id`, never here or in a brief. */
export interface Connection {
  /** Stable slug, e.g. "linear-personal". Also the keyring entry key. */
  id: string;
  provider: Provider;
  label: string;
  /** Jira cloud instance / GitHub Enterprise base URL. */
  baseUrl?: string | null;
  /** e.g. a Jira account email — never the token. */
  account?: string | null;
  /** GitHub only: scope this connection's feeds to these `owner/name` repos.
   *  Empty/absent → every repo the token can reach (so a brief never pulls in
   *  commits/PRs from unrelated projects). */
  repos?: string[] | null;
  /** NeuroSkill only: local daemon endpoint for the `label` write
   *  (default http://127.0.0.1:18444). A ws:// value is accepted for
   *  back-compat. Overridable for WSL2↔Windows-host. */
  wsUrl?: string | null;
  /** NeuroSkill only: directory holding activity.sqlite / labels.sqlite
   *  (default the WSL-translated AppData path). Not a secret. */
  dataDir?: string | null;
  /** NeuroSkill only: path to the daemon's bearer-token file
   *  (default the OS …/skill/daemon/auth.token). On WSL2 the token lives on
   *  the Windows host, so this is overridable. Read at call time; not a
   *  secret WAID stores. */
  tokenPath?: string | null;
}

/** A brief's reference to a connection plus a provider-specific selector. */
export interface BriefIntegration {
  /** References Connection.id. */
  connection: string;
  /** e.g. "tasks" | "notifications" | "pulls" | "commits" (provider-specific). */
  kind: string;
  query?: string | null;
  limit?: number | null;
  /** `false` opts the feed out of background auto-sync; absent means on. */
  poll?: boolean | null;
}

/** A normalized task / notification from any provider (see fetch_integration). */
export interface IntegrationItem {
  id: string;
  title: string;
  /** Opened via the existing opener plugin. */
  url: string;
  status?: string | null;
  assignee?: string | null;
  /** ISO timestamp; render with time.ts. */
  updatedAt?: string | null;
  /** "task" | "notification". */
  kind: string;
  /** Provider extras (priority, project, …). */
  meta?: Record<string, string>;
}

/** Local (no-LLM) rollup of an integration fetch. */
export interface IntegrationSummary {
  total: number;
  byStatus: Record<string, number>;
  overdue?: number | null;
  /** Items updated within the last 7 days. */
  updatedRecently?: number | null;
}

/** Rate-limit state a provider reported on its last response. */
export interface RateInfo {
  remaining?: number | null;
  /** RFC3339. */
  resetAt?: string | null;
  /** Server-requested minimum poll interval (GitHub `X-Poll-Interval`). */
  minIntervalMs?: number | null;
}

/** Where a fetch's items came from: a real request, the backend's in-memory
 *  feed cache, or a 304 that re-served the cached items. */
export type ServedFrom = "network" | "cache" | "notModified";

/** What fetch_integration returns: normalized items + when + the rollup, plus
 *  the bookkeeping auto-sync budgets with. */
export interface IntegrationFetch {
  items: IntegrationItem[];
  fetchedAt: string;
  summary: IntegrationSummary;
  /** Session-salted hash of the credential; feeds sharing a token share it. */
  credentialId?: string | null;
  /** Requests actually made (0 for a cache hit). */
  cost?: number;
  servedFrom?: ServedFrom;
  rate?: RateInfo | null;
}

export interface Brief {
  /** Absolute path to the .md file on disk. */
  path: string;
  fileName: string;
  name: string;
  status?: string | null;
  description?: string | null;
  tags: string[];
  links: Link[];
  webhooks: Webhook[];
  /** Explicit sync sources (GitHub repos come from `links`). */
  sources: SyncSource[];
  /** This brief's own PM-integration connections (metadata; never tokens). */
  connections: Connection[];
  /** PM-integration selectors (connection id + selector; never tokens). */
  integrations: BriefIntegration[];
  lastOpened?: string | null;
  /** When the managed sync block was last written (from the body, not
   *  frontmatter); null if never synced. */
  lastSynced?: string | null;
  /** Markdown body (everything after the frontmatter). Shown by the renderer. */
  body: string;
  /** The entire raw file, frontmatter included. Shown/saved by edit mode. */
  raw: string;
}

export interface WebhookResult {
  status: number;
  ok: boolean;
  body: string;
}

/** LLM synthesis settings (see get_llm_settings / set_llm_settings).
 *  `llmProvider` of "ollama" | "anthropic" | "openai" enables the synthesis
 *  agent; null disables it. "openai" is the OpenAI-compatible wire protocol
 *  (OpenRouter, Groq, LM Studio, llama.cpp, vLLM, …), not the company. Secrets
 *  (API keys) live in the OS keyring, not here. */
export interface LlmSettings {
  llmProvider?: string | null;
  ollamaUrl?: string | null;
  ollamaModel?: string | null;
  anthropicModel?: string | null;
  /** Base URL incl. any `/v1`; the backend appends `/chat/completions`. */
  openaiUrl?: string | null;
  openaiModel?: string | null;
}

/** Guided-interview answers for the brief-bootstrap chooser (see
 *  bootstrap_from_answers). Mirrors the Rust `BootstrapAnswers` struct. */
export interface BootstrapAnswers {
  /** "What is this project?" */
  summary: string;
  /** "What's the goal / definition of done?" */
  goal?: string | null;
  /** "Anything else / current state in your words." */
  notes?: string | null;
}

/** Whether the briefs dir lives in an Obsidian vault (see get_vault_info). */
export interface VaultInfo {
  isVault: boolean;
  /** Vault folder name — the `vault` param of an obsidian:// deep link. */
  name?: string | null;
  /** Absolute path of the vault root (the folder containing `.obsidian/`). */
  root?: string | null;
}
