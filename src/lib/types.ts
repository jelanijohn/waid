// Mirrors the structs returned by the Rust backend (see src-tauri/src/commands.rs).
// Field names are camelCase because the Rust structs use `#[serde(rename_all = "camelCase")]`.

export interface Link {
  label: string;
  url: string;
}

export interface Webhook {
  label: string;
  url: string;
  method: string;
  body?: string | null;
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
 *  `llmProvider` of "ollama" | "anthropic" enables the synthesis agent; null
 *  disables it. Secrets (the Anthropic API key) live in the OS keyring, not here. */
export interface LlmSettings {
  llmProvider?: string | null;
  ollamaUrl?: string | null;
  ollamaModel?: string | null;
  anthropicModel?: string | null;
}

/** Whether the briefs dir lives in an Obsidian vault (see get_vault_info). */
export interface VaultInfo {
  isVault: boolean;
  /** Vault folder name — the `vault` param of an obsidian:// deep link. */
  name?: string | null;
  /** Absolute path of the vault root (the folder containing `.obsidian/`). */
  root?: string | null;
}
