// Launch actions shared by the dashboard (ProjectDetail, Titlebar) and the
// widget, so the two surfaces can't drift apart.

import type { Brief, Webhook } from "$lib/types";
import { projects } from "$lib/stores/projects.svelte";
import { session } from "$lib/stores/session.svelte";
import { toasts } from "$lib/stores/toasts.svelte";
import { openExternal, fireWebhook } from "$lib/tauri";

/** WSL can't reach the Windows host without a URL handler; hint at the fix. */
export function wslHint(): string {
  return projects.isWsl
    ? " WSL needs a URL handler — install wslu so wslview forwards links to Windows."
    : "";
}

/** Pick a Material glyph for a launch link based on its label. */
export function iconForLink(label: string): string {
  const l = label.toLowerCase();
  if (l.includes("github")) return "code";
  if (l.includes("docs") || l.includes("tauri")) return "menu_book";
  if (l.includes("obsidian")) return "hub";
  if (l.includes("claude")) return "auto_awesome";
  return "north_east";
}

/** Open a brief's link. Opening a link IS launching into work — start the
 *  labeled session once. */
export async function launchLink(brief: Brief, url: string): Promise<void> {
  void session.start(brief);
  try {
    await openExternal(url);
  } catch (e) {
    toasts.error(`Could not open link (${e}).${wslHint()}`);
  }
}

/** Fire a brief's webhook (also starts the labeled session). */
export async function launchWebhook(brief: Brief, hook: Webhook): Promise<void> {
  void session.start(brief);
  try {
    const res = await fireWebhook(brief.path, hook);
    if (res.ok) {
      toasts.success(`${hook.label || "Webhook"} → ${res.status}`);
    } else {
      toasts.error(`${hook.label || "Webhook"} → ${res.status}`);
    }
  } catch (e) {
    toasts.error(`${hook.label || "Webhook"} failed: ${e}`);
  }
}

/** Sync every syncable brief (then synthesize when a provider is configured),
 *  with all of Sync all's toasts. The caller owns its own `syncing` flag. */
export async function syncAllProjects(): Promise<void> {
  try {
    const outcomes = await projects.syncAll();
    const failed = outcomes.filter((o) => !o.ok);
    const synced = outcomes.length - failed.length;
    // When a provider is configured, also synthesize each syncable brief.
    if (projects.llmProvider) {
      try {
        await projects.synthesizeAll();
      } catch (e) {
        toasts.error(`Synthesis failed: ${e}`);
      }
    }
    if (outcomes.length === 0) {
      toasts.push("Nothing to sync — no briefs have a GitHub link or source.", "info");
    } else if (failed.length === 0) {
      toasts.success(`Synced ${synced} project${synced === 1 ? "" : "s"}`);
    } else {
      toasts.error(`Synced ${synced}, ${failed.length} failed (${failed[0].name}: ${failed[0].error})`);
    }
  } catch (e) {
    toasts.error(`Sync all failed: ${e}`);
  }
}
