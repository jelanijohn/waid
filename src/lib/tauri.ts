// Thin wrappers around Tauri `invoke` calls and plugins, so components never
// touch the raw command names / channels directly.

import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Brief, VaultInfo, WebhookResult } from "./types";

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

export const getBriefsDir = () => invoke<string>("get_briefs_dir");

export const setBriefsDir = (dir: string) => invoke<string>("set_briefs_dir", { dir });

export const getVaultInfo = () => invoke<VaultInfo>("get_vault_info");

export const isWsl = () => invoke<boolean>("is_wsl");

export const createBrief = (name: string) => invoke<Brief>("create_brief", { name });

/** Open a URL in the user's default browser. */
export const openExternal = (url: string) => openUrl(url);

/** Show a native folder picker; returns the chosen path or null if cancelled. */
export async function pickDirectory(): Promise<string | null> {
  const selected = await openDialog({ directory: true, multiple: false });
  if (typeof selected === "string") return selected;
  return null;
}
