// Thin wrappers around Tauri `invoke` calls and plugins, so components never
// touch the raw command names / channels directly.

import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Brief, SyncOutcome, VaultInfo, WebhookResult } from "./types";

export const listBriefs = () => invoke<Brief[]>("list_briefs");

export const readBrief = (path: string) => invoke<Brief>("read_brief", { path });

export const saveBrief = (path: string, content: string) =>
  invoke<Brief>("save_brief", { path, content });

export const touchBrief = (path: string) => invoke<Brief>("touch_brief", { path });

export const appendCapture = (path: string, note: string) =>
  invoke<Brief>("append_capture", { path, note });

export const fireWebhook = (
  url: string,
  method: string,
  body?: string | null,
) => invoke<WebhookResult>("fire_webhook", { url, method, body: body ?? null });

/** Refresh a brief's managed sync block from its linked integrations. */
export const syncBrief = (path: string) => invoke<Brief>("sync_brief", { path });

/** Sync every brief that has a source; returns per-brief outcomes. */
export const syncAll = () => invoke<SyncOutcome[]>("sync_all");

export const getBriefsDir = () => invoke<string>("get_briefs_dir");

export const setBriefsDir = (dir: string) => invoke<string>("set_briefs_dir", { dir });

export const getVaultInfo = () => invoke<VaultInfo>("get_vault_info");

export const isWsl = () => invoke<boolean>("is_wsl");

export const createBrief = (name: string) => invoke<Brief>("create_brief", { name });

// --- Secrets (OS keyring) --------------------------------------------------
// Tokens for authenticated integrations live in the platform keychain, never
// in settings or env. Keys mirror the SECRET_* constants in commands.rs.

/** Keyring key for the GitHub token used by brief sync (private repos). */
export const SECRET_GITHUB_TOKEN = "github.token";

/** Store (or replace) a secret in the OS keyring. */
export const setSecret = (key: string, value: string) =>
  invoke<void>("set_secret", { key, value });

/** Read a secret from the OS keyring (null when not set). */
export const getSecret = (key: string) => invoke<string | null>("get_secret", { key });

/** Remove a secret from the OS keyring. */
export const deleteSecret = (key: string) => invoke<void>("delete_secret", { key });

/** Whether a secret is stored, without returning its value. */
export const hasSecret = (key: string) => invoke<boolean>("has_secret", { key });

/** Open a URL in the user's default browser. */
export const openExternal = (url: string) => openUrl(url);

/** Show a native folder picker; returns the chosen path or null if cancelled. */
export async function pickDirectory(): Promise<string | null> {
  const selected = await openDialog({ directory: true, multiple: false });
  if (typeof selected === "string") return selected;
  return null;
}
