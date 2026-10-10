// Thin wrappers around Tauri `invoke` calls and plugins, so components never
// touch the raw command names / channels directly.

import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  BootstrapAnswers,
  Brief,
  BriefIntegration,
  Connection,
  IntegrationFetch,
  LlmSettings,
  SyncOutcome,
  VaultInfo,
  Webhook,
  WebhookResult,
} from "./types";

export const listBriefs = () => invoke<Brief[]>("list_briefs");

export const readBrief = (path: string) => invoke<Brief>("read_brief", { path });

export const saveBrief = (path: string, content: string) =>
  invoke<Brief>("save_brief", { path, content });

export const touchBrief = (path: string) => invoke<Brief>("touch_brief", { path });

/** Set a brief's `status` frontmatter key; returns the reparsed brief. */
export const setBriefStatus = (path: string, status: string) =>
  invoke<Brief>("set_brief_status", { path, status });

export const appendCapture = (path: string, note: string) =>
  invoke<Brief>("append_capture", { path, note });

// fire_webhook resolves headers + the keyring secret server-side, so it needs
// the owning brief's path plus the whole Webhook object.
export const fireWebhook = (path: string, webhook: Webhook) =>
  invoke<WebhookResult>("fire_webhook", { path, webhook });

/** Add/update a webhook on a brief (identity = slug id); empty secret keeps the existing one. */
export const saveBriefWebhook = (path: string, webhook: Webhook, secret: string) =>
  invoke<Brief>("save_brief_webhook", { path, webhook, secret });

/** Remove a webhook (by slug id) + its keyring secret. */
export const deleteBriefWebhook = (path: string, id: string) =>
  invoke<Brief>("delete_brief_webhook", { path, id });

/** Refresh a brief's managed sync block from its linked integrations. */
export const syncBrief = (path: string) => invoke<Brief>("sync_brief", { path });

/** Sync every brief that has a source; returns per-brief outcomes. */
export const syncAll = () => invoke<SyncOutcome[]>("sync_all");

/** Regenerate a brief's deterministic `## Mind State` region from NeuroSkill's
 *  local EEG/label data (read-only). Returns the rewritten brief. */
export const syncMindState = (path: string) => invoke<Brief>("sync_mind_state", { path });

/** Fire a NeuroSkill `waid:brief=<slug>:(start|end)` session label over the local
 *  WebSocket. Best-effort: a no-op for briefs without a NeuroSkill connection. */
export const markBriefSession = (path: string, phase: "start" | "end") =>
  invoke<void>("mark_brief_session", { path, phase });

// --- LLM synthesis (the brief-synthesis agent) -----------------------------

/** Synthesize a brief's Current State + Open Questions via the configured LLM.
 *  (The backend takes an AppHandle; the JS side passes only the path.) */
export const synthesizeBrief = (path: string) => invoke<Brief>("synthesize_brief", { path });

/** Synthesize every syncable brief; returns per-brief outcomes (mirrors syncAll). */
export const synthesizeAll = () => invoke<SyncOutcome[]>("synthesize_all");

/** List models available from an Ollama server (for the settings dropdown). */
export const listOllamaModels = (baseUrl?: string | null) =>
  invoke<string[]>("list_ollama_models", { baseUrl: baseUrl ?? null });

/** Read the LLM synthesis settings (provider + model config; never secrets). */
export const getLlmSettings = () => invoke<LlmSettings>("get_llm_settings");

/** Persist the LLM synthesis settings. */
export const setLlmSettings = (settings: LlmSettings) =>
  invoke<void>("set_llm_settings", { settings });

export const getBriefsDir = () => invoke<string>("get_briefs_dir");

export const setBriefsDir = (dir: string) => invoke<string>("set_briefs_dir", { dir });

/** Persist the sidebar's manual order (all brief paths, top to bottom). An
 *  empty list clears it and restores most-recently-opened sorting. */
export const setBriefOrder = (paths: string[]) => invoke<void>("set_brief_order", { paths });

export const getVaultInfo = () => invoke<VaultInfo>("get_vault_info");

export const isWsl = () => invoke<boolean>("is_wsl");

export const createBrief = (name: string) => invoke<Brief>("create_brief", { name });

// --- Brief bootstrap (initial-brief generation) ----------------------------
// Each returns a *proposed raw file string* (frontmatter + body) WITHOUT writing
// to disk. The frontend drops it into edit mode; the user's Save is the commit
// gate. Deterministic metadata always; AI body only when a provider is set.

/** Draft a brief from a local folder/repo (README + manifest + file tree). */
export const bootstrapFromFolder = (path: string, dir: string) =>
  invoke<string>("bootstrap_from_folder", { path, dir });

/** Draft a brief from a GitHub repo URL (description/topics/README). */
export const bootstrapFromGithub = (path: string, url: string) =>
  invoke<string>("bootstrap_from_github", { path, url });

/** Draft a brief from guided-interview answers (composes locally w/o a provider). */
export const bootstrapFromAnswers = (path: string, answers: BootstrapAnswers) =>
  invoke<string>("bootstrap_from_answers", { path, answers });

/** Normalize a pasted bootstrap brief: lift its Current State / Open Questions
 *  sections into WAID's app-owned marker regions so a later Refresh regenerates
 *  them in place instead of duplicating. Returns the proposed raw (edit-mode). */
export const normalizeBootstrapPaste = (pasted: string) =>
  invoke<string>("normalize_bootstrap_paste", { pasted });

// --- Secrets (OS keyring) --------------------------------------------------
// Tokens for authenticated integrations live in the platform keychain, never
// in settings or env. Keys mirror the SECRET_* constants in commands.rs.

/** Keyring key for the GitHub token used by brief sync (private repos). */
export const SECRET_GITHUB_TOKEN = "github.token";

/** Keyring key for the Anthropic API key used by the synthesis agent. */
export const SECRET_ANTHROPIC_API_KEY = "anthropic.api_key";
/** Prefix of the keyring key for the OpenAI-compatible endpoint's API key
 *  (optional — local servers need none). The full key is scoped to the
 *  endpoint's origin; use `openaiKeySecret(url)`. */
export const SECRET_OPENAI_API_KEY = "openai.api_key";

/** Keyring key for the API key of the OpenAI-compatible endpoint at `url`:
 *  `openai.api_key:<origin>`, so editing the path keeps the key but switching
 *  hosts never sends a saved cloud key elsewhere. Mirrors the Rust
 *  `openai_key_secret_name` in commands.rs — keep the two in sync. */
export function openaiKeySecret(url: string): string {
  const base = url.trim();
  let scope = base;
  try {
    const origin = new URL(base).origin;
    if (origin !== "null") scope = origin;
  } catch {
    // not a URL — fall back to the raw text, like the backend
  }
  return `${SECRET_OPENAI_API_KEY}:${scope}`;
}

/** Keyring keys for the user's bring-your-own Google OAuth *Desktop* client,
 *  used by the Gmail provider's connect flow (see connectGmail). */
export const SECRET_GMAIL_CLIENT_ID = "gmail.client_id";
export const SECRET_GMAIL_CLIENT_SECRET = "gmail.client_secret";

/** Store (or replace) a secret in the OS keyring. */
export const setSecret = (key: string, value: string) =>
  invoke<void>("set_secret", { key, value });

/** Read a secret from the OS keyring (null when not set). */
export const getSecret = (key: string) => invoke<string | null>("get_secret", { key });

/** Remove a secret from the OS keyring. */
export const deleteSecret = (key: string) => invoke<void>("delete_secret", { key });

/** Whether a secret is stored, without returning its value. */
export const hasSecret = (key: string) => invoke<boolean>("has_secret", { key });

// --- PM integrations: per-brief connections + selectors --------------------
// Connections belong to a single brief: their metadata lives in that brief's
// frontmatter, the token in the OS keyring (keyed by brief path + id). These
// mutating calls return the reparsed brief so the store can upsert it.

/** Add/update a connection on a brief. An empty token keeps the existing one. */
export const saveBriefConnection = (path: string, connection: Connection, token: string) =>
  invoke<Brief>("save_brief_connection", { path, connection, token });

/** Remove a connection (and any selectors referencing it) from a brief. */
export const deleteBriefConnection = (path: string, id: string) =>
  invoke<Brief>("delete_brief_connection", { path, id });

/** Add/update an integration selector (keyed by connection + kind + query) on a brief. */
export const saveBriefIntegration = (path: string, integration: BriefIntegration) =>
  invoke<Brief>("save_brief_integration", { path, integration });

/** Remove an integration selector (by connection + kind + query) from a brief. */
export const deleteBriefIntegration = (
  path: string,
  connection: string,
  kind: string,
  query?: string | null,
) => invoke<Brief>("delete_brief_integration", { path, connection, kind, query: query ?? null });

/** Verify a brief connection's saved token against its provider. */
export const testBriefConnection = (path: string, id: string) =>
  invoke<void>("test_brief_connection", { path, id });

/** Run the Google OAuth desktop loopback flow for one Gmail account. Opens the
 *  browser, captures the consent, stores the account-scoped grant in the keyring,
 *  and returns the connected account's email (set it as Connection.account). */
export const connectGmail = () => invoke<string>("connect_gmail");

/** Generate an LLM digest of a brief's live integration items (display-only). */
export const digestIntegrations = (path: string) =>
  invoke<string>("digest_integrations", { path });

/** Turn a plain-English description into a Gmail search query via the configured
 *  synthesis LLM. Returns a single query line to drop into a feed's filter. */
export const generateGmailQuery = (prompt: string) =>
  invoke<string>("generate_gmail_query", { prompt });

/** Turn a plain-English description into a Slack search query via the configured
 *  synthesis LLM. Returns a single query line to drop into a feed's filter. */
export const generateSlackQuery = (prompt: string) =>
  invoke<string>("generate_slack_query", { prompt });

/** Generate a cross-brief "morning briefing" across all briefs' integrations. */
export const morningBriefing = () => invoke<string>("morning_briefing");

/** A rejected `fetch_integration`, typed so auto-sync can tell a rate limit
 *  (and how long to wait) from any other failure. `toString()` is the plain
 *  message, so `String(e)` / `${e}` read exactly as before. */
export class IntegrationFetchError extends Error {
  kind: "rateLimited" | "auth" | "http" | "network" | "other";
  retryAfterMs?: number;
  status?: number;

  constructor(raw: unknown) {
    const o = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
    super(o && typeof o.message === "string" ? o.message : String(raw));
    this.name = "IntegrationFetchError";
    const kind = o?.kind;
    this.kind =
      kind === "rateLimited" || kind === "auth" || kind === "http" || kind === "network" ? kind : "other";
    if (typeof o?.retryAfterMs === "number") this.retryAfterMs = o.retryAfterMs;
    if (typeof o?.status === "number") this.status = o.status;
  }

  toString(): string {
    return this.message;
  }
}

/** Fetch live items for one of a brief's integration selectors (token internal).
 *  `maxAgeMs` lets the backend answer from its in-memory feed cache when its
 *  copy is at most that old (shared across briefs on the same credential);
 *  omitted or 0 always asks the provider. Rejects with `IntegrationFetchError`. */
export const fetchIntegration = (
  path: string,
  connectionId: string,
  kind: string,
  query?: string | null,
  limit?: number | null,
  maxAgeMs?: number,
) =>
  invoke<IntegrationFetch>("fetch_integration", {
    path,
    connectionId,
    kind,
    query: query ?? null,
    limit: limit ?? null,
    maxAgeMs: maxAgeMs ? Math.round(maxAgeMs) : null,
  }).catch((e: unknown) => {
    throw new IntegrationFetchError(e);
  });

/** Open a URL in the user's default browser. */
export const openExternal = (url: string) => openUrl(url);

/** Show a native folder picker; returns the chosen path or null if cancelled. */
export async function pickDirectory(): Promise<string | null> {
  const selected = await openDialog({ directory: true, multiple: false });
  if (typeof selected === "string") return selected;
  return null;
}
