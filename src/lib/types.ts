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
  lastOpened?: string | null;
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

/** Whether the briefs dir lives in an Obsidian vault (see get_vault_info). */
export interface VaultInfo {
  isVault: boolean;
  /** Vault folder name — the `vault` param of an obsidian:// deep link. */
  name?: string | null;
  /** Absolute path of the vault root (the folder containing `.obsidian/`). */
  root?: string | null;
}
