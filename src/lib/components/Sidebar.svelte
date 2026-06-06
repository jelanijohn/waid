<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { relativeTime } from "$lib/time";
  import {
    createBrief,
    pickDirectory,
    setBriefsDir,
    setSecret,
    deleteSecret,
    hasSecret,
    SECRET_GITHUB_TOKEN,
    SECRET_ANTHROPIC_API_KEY,
    SECRET_GMAIL_CLIENT_ID,
    SECRET_GMAIL_CLIENT_SECRET,
    getLlmSettings,
    setLlmSettings,
    listOllamaModels,
  } from "$lib/tauri";
  import { settings, ACCENTS, type SidebarStyle, type Density } from "$lib/stores/settings.svelte";
  import { STATUS_ORDER, STATUS_LABEL, statusColor } from "$lib/status";
  import StatusPill from "./StatusPill.svelte";
  import Icon from "./Icon.svelte";
  import BrandMark from "./BrandMark.svelte";
  import BriefingModal from "./BriefingModal.svelte";

  let creating = $state(false);
  let briefingOpen = $state(false);
  let newName = $state("");
  let searchEl = $state<HTMLInputElement>();
  let settingsOpen = $state(false);
  let syncingAll = $state(false);

  // GitHub token (OS keyring) — managed from the settings popover.
  let ghToken = $state("");
  let ghStored = $state(false);
  let ghBusy = $state(false);

  // Gmail OAuth client (bring-your-own Desktop client; OS keyring) — managed from
  // the settings popover. Both id + secret are required before Connect works.
  let gmailClientId = $state("");
  let gmailClientSecret = $state("");
  let gmailClientStored = $state(false);
  let gmailBusy = $state(false);

  // LLM synthesis settings — managed from the settings popover.
  // Provider "" means disabled; "ollama" / "anthropic" select a backend.
  let llmProvider = $state<"" | "ollama" | "anthropic">("");
  let ollamaUrl = $state("");
  let ollamaModel = $state("");
  let ollamaModels = $state<string[]>([]);
  let ollamaError = $state("");
  let anthropicModel = $state("");
  let anthropicKey = $state("");
  let anthropicStored = $state(false);
  let llmBusy = $state(false);

  const OLLAMA_DEFAULT_URL = "http://localhost:11434";

  onMount(() => {
    // ⌘/Ctrl+F focuses the search box; ⌘/Ctrl+N starts a new project.
    const onKey = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && k === "f") {
        e.preventDefault();
        searchEl?.focus();
        searchEl?.select();
      } else if ((e.metaKey || e.ctrlKey) && k === "n") {
        e.preventDefault();
        startNew();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Projects grouped by status (compact + rocks views), in priority order with
  // any custom statuses appended.
  let groups = $derived.by(() => {
    const map = new Map<string, Brief[]>();
    for (const b of projects.filtered) {
      const s = (b.status ?? "").toLowerCase() || "other";
      const bucket = map.get(s);
      if (bucket) bucket.push(b);
      else map.set(s, [b]);
    }
    const rank = (s: string) => {
      const i = (STATUS_ORDER as readonly string[]).indexOf(s);
      return i === -1 ? 99 : i;
    };
    return [...map.entries()]
      .sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]))
      .map(([status, items]) => ({ status, items }));
  });

  function groupLabel(status: string): string {
    return STATUS_LABEL[status] ?? status.charAt(0).toUpperCase() + status.slice(1);
  }

  function toggleStatus(status: string) {
    projects.statusFilter = projects.statusFilter === status ? null : status;
  }

  function startNew() {
    settingsOpen = false;
    newName = "";
    creating = true;
  }

  function toggleSettings() {
    settingsOpen = !settingsOpen;
    if (settingsOpen) {
      ghToken = "";
      gmailClientId = "";
      gmailClientSecret = "";
      refreshTokenStatus();
      refreshGmailClientStatus();
      loadLlmSettings();
    }
  }

  async function refreshGmailClientStatus() {
    try {
      gmailClientStored = await hasSecret(SECRET_GMAIL_CLIENT_ID);
    } catch {
      gmailClientStored = false;
    }
  }

  async function saveGmailClient() {
    const id = gmailClientId.trim();
    const secret = gmailClientSecret.trim();
    if (!id || !secret || gmailBusy) return;
    gmailBusy = true;
    try {
      await setSecret(SECRET_GMAIL_CLIENT_ID, id);
      await setSecret(SECRET_GMAIL_CLIENT_SECRET, secret);
      gmailClientId = "";
      gmailClientSecret = "";
      gmailClientStored = true;
      toasts.success("Google OAuth client saved to keychain");
    } catch (e) {
      toasts.error(`Could not save Google client: ${e}`);
    } finally {
      gmailBusy = false;
    }
  }

  async function clearGmailClient() {
    if (gmailBusy) return;
    gmailBusy = true;
    try {
      await deleteSecret(SECRET_GMAIL_CLIENT_ID);
      await deleteSecret(SECRET_GMAIL_CLIENT_SECRET);
      gmailClientStored = false;
      toasts.success("Google OAuth client removed");
    } catch (e) {
      toasts.error(`Could not remove Google client: ${e}`);
    } finally {
      gmailBusy = false;
    }
  }

  async function loadLlmSettings() {
    try {
      const s = await getLlmSettings();
      llmProvider = (s.llmProvider as "" | "ollama" | "anthropic") ?? "";
      ollamaUrl = s.ollamaUrl ?? OLLAMA_DEFAULT_URL;
      ollamaModel = s.ollamaModel ?? "";
      anthropicModel = s.anthropicModel ?? "";
    } catch {
      // Settings unreadable — fall back to disabled.
      llmProvider = "";
    }
    anthropicKey = "";
    try {
      anthropicStored = await hasSecret(SECRET_ANTHROPIC_API_KEY);
    } catch {
      anthropicStored = false;
    }
    if (llmProvider === "ollama") fetchOllamaModels();
  }

  /** Persist the current synthesis settings and refresh the store's provider
   *  flag (so the Refresh button reflects the change immediately). */
  async function saveLlmSettings() {
    try {
      await setLlmSettings({
        llmProvider: llmProvider || null,
        ollamaUrl: ollamaUrl.trim() || null,
        ollamaModel: ollamaModel.trim() || null,
        anthropicModel: anthropicModel.trim() || null,
      });
      await projects.refreshLlmProvider();
    } catch (e) {
      toasts.error(`Could not save AI settings: ${e}`);
    }
  }

  async function onProviderChange(value: string) {
    llmProvider = value as "" | "ollama" | "anthropic";
    await saveLlmSettings();
    if (llmProvider === "ollama") fetchOllamaModels();
  }

  async function fetchOllamaModels() {
    ollamaError = "";
    try {
      ollamaModels = await listOllamaModels(ollamaUrl.trim() || OLLAMA_DEFAULT_URL);
      if (ollamaModels.length === 0) {
        ollamaError = "No models found — pull one with `ollama pull …`.";
      } else if (!ollamaModel && ollamaModels.length) {
        // Default the selection to the first available model.
        ollamaModel = ollamaModels[0];
        await saveLlmSettings();
      }
    } catch (e) {
      ollamaModels = [];
      ollamaError = `Could not reach Ollama: ${e}`;
    }
  }

  async function saveAnthropicKey() {
    const value = anthropicKey.trim();
    if (!value || llmBusy) return;
    llmBusy = true;
    try {
      await setSecret(SECRET_ANTHROPIC_API_KEY, value);
      anthropicKey = "";
      anthropicStored = true;
      toasts.success("Anthropic API key saved to keychain");
    } catch (e) {
      toasts.error(`Could not save key: ${e}`);
    } finally {
      llmBusy = false;
    }
  }

  async function clearAnthropicKey() {
    if (llmBusy) return;
    llmBusy = true;
    try {
      await deleteSecret(SECRET_ANTHROPIC_API_KEY);
      anthropicStored = false;
      toasts.success("Anthropic API key removed");
    } catch (e) {
      toasts.error(`Could not remove key: ${e}`);
    } finally {
      llmBusy = false;
    }
  }

  async function refreshTokenStatus() {
    try {
      ghStored = await hasSecret(SECRET_GITHUB_TOKEN);
    } catch {
      // Keyring may be unavailable (e.g. no daemon on WSL) — treat as "not set".
      ghStored = false;
    }
  }

  async function saveToken() {
    const value = ghToken.trim();
    if (!value || ghBusy) return;
    ghBusy = true;
    try {
      await setSecret(SECRET_GITHUB_TOKEN, value);
      ghToken = "";
      ghStored = true;
      toasts.success("GitHub token saved to keychain");
    } catch (e) {
      toasts.error(`Could not save token: ${e}`);
    } finally {
      ghBusy = false;
    }
  }

  async function clearToken() {
    if (ghBusy) return;
    ghBusy = true;
    try {
      await deleteSecret(SECRET_GITHUB_TOKEN);
      ghStored = false;
      toasts.success("GitHub token removed");
    } catch (e) {
      toasts.error(`Could not remove token: ${e}`);
    } finally {
      ghBusy = false;
    }
  }

  async function syncAllBriefs() {
    if (syncingAll) return;
    syncingAll = true;
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
    } finally {
      syncingAll = false;
    }
  }

  async function changeFolder() {
    try {
      const dir = await pickDirectory();
      if (!dir) return;
      await setBriefsDir(dir);
      await projects.load();
      toasts.success("Briefs folder updated");
    } catch (e) {
      toasts.error(`Could not change folder: ${e}`);
    }
  }

  async function submitNew() {
    const name = newName.trim();
    if (!name) {
      creating = false;
      return;
    }
    try {
      const brief = await createBrief(name);
      await projects.load();
      // New projects start as a stub — pop Generate immediately on select.
      projects.bootstrapPath = brief.path;
      await projects.select(brief.path);
      newName = "";
      creating = false;
    } catch (e) {
      toasts.error(`Could not create project: ${e}`);
    }
  }

  const SIDEBAR_STYLES: { value: SidebarStyle; label: string }[] = [
    { value: "rows", label: "Rows" },
    { value: "compact", label: "Compact" },
    { value: "rocks", label: "Rocks" },
  ];
  const DENSITIES: { value: Density; label: string }[] = [
    { value: "comfortable", label: "Comfortable" },
    { value: "compact", label: "Compact" },
  ];
</script>

<aside
  class="flex h-screen w-[270px] shrink-0 flex-col border-r bg-[var(--side-bg)]"
  style="border-color: var(--border);"
>
  <!-- Brand header -->
  <header
    class="flex items-center justify-between border-b px-4 py-[13px]"
    style="border-color: var(--border);"
  >
    <div class="flex items-center gap-[10px]">
      <BrandMark size={22} />
      <div>
        <div class="text-[16px] font-bold leading-none tracking-[-0.01em] text-[var(--fg)]">WAID</div>
        <div class="mt-[2px] text-[10.5px] text-[var(--fg3)]">What Am I Doing?</div>
      </div>
    </div>
    <div class="relative flex items-center gap-1">
      {#if projects.llmProvider}
        <button
          class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
          title="Morning briefing across all projects"
          aria-label="Morning briefing"
          onclick={() => (briefingOpen = true)}
        >
          <Icon name="wb_sunny" size={17} />
        </button>
      {/if}
      <button
        class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
        title="Sync all projects"
        aria-label="Sync all projects"
        onclick={syncAllBriefs}
        disabled={syncingAll}
      >
        <Icon name="sync" size={17} class={syncingAll ? "spin" : ""} />
      </button>
      <button
        class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
        title="View & appearance"
        aria-label="View & appearance"
        onclick={toggleSettings}
      >
        <Icon name="tune" size={17} />
      </button>
      <button
        class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
        title="Toggle theme"
        aria-label="Toggle theme"
        onclick={() => settings.toggleDark()}
      >
        <Icon name={settings.dark ? "light_mode" : "dark_mode"} size={17} />
      </button>

      {#if settingsOpen}
        <!-- click-away layer -->
        <button
          class="fixed inset-0 z-40 cursor-default"
          aria-label="Close settings"
          onclick={() => (settingsOpen = false)}
        ></button>
        <div
          class="anim-pop absolute right-0 top-[34px] z-50 max-h-[calc(100vh-56px)] w-[232px] overflow-y-auto overscroll-contain rounded-xl border p-3 shadow-[0_16px_40px_rgba(10,20,40,0.28)]"
          style="background: var(--bg); border-color: var(--border);"
        >
          <!-- Project cards -->
          <div class="mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
            Project cards
          </div>
          <div class="mb-3 flex rounded-lg border p-[2px]" style="border-color: var(--border);">
            {#each SIDEBAR_STYLES as opt (opt.value)}
              <button
                class="flex-1 rounded-md px-2 py-1 text-[11.5px] font-medium transition-colors {settings.sidebarStyle ===
                opt.value
                  ? 'bg-[var(--accent)] text-white'
                  : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                onclick={() => settings.setSidebarStyle(opt.value)}
              >
                {opt.label}
              </button>
            {/each}
          </div>

          <!-- Accent -->
          <div class="mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
            Accent
          </div>
          <div class="mb-3 flex gap-2">
            {#each ACCENTS as color (color)}
              <button
                class="h-6 w-6 rounded-full transition-transform hover:scale-110 {settings.accent ===
                color
                  ? 'ring-2 ring-offset-2'
                  : ''}"
                style="background: {color}; --tw-ring-color: {color}; --tw-ring-offset-color: var(--bg);"
                title={color}
                aria-label="Accent {color}"
                onclick={() => settings.setAccent(color)}
              ></button>
            {/each}
          </div>

          <!-- Density -->
          <div class="mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
            Density
          </div>
          <div class="flex rounded-lg border p-[2px]" style="border-color: var(--border);">
            {#each DENSITIES as opt (opt.value)}
              <button
                class="flex-1 rounded-md px-2 py-1 text-[11.5px] font-medium transition-colors {settings.density ===
                opt.value
                  ? 'bg-[var(--accent)] text-white'
                  : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                onclick={() => settings.setDensity(opt.value)}
              >
                {opt.label}
              </button>
            {/each}
          </div>

          <!-- Sync -->
          <div
            class="mb-2 mt-3 border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
            style="border-color: var(--border);"
          >
            Sync
          </div>
          <button
            class="flex w-full items-center justify-between gap-2 text-left"
            onclick={() => settings.setAutoSyncOnOpen(!settings.autoSyncOnOpen)}
          >
            <span class="text-[11.5px] text-[var(--fg2)]">Auto-sync on open</span>
            <span
              class="relative h-[18px] w-[30px] shrink-0 rounded-full transition-colors"
              style="background: {settings.autoSyncOnOpen ? 'var(--accent)' : 'var(--border)'};"
            >
              <span
                class="absolute top-[2px] h-[14px] w-[14px] rounded-full bg-white transition-[left]"
                style="left: {settings.autoSyncOnOpen ? '14px' : '2px'};"
              ></span>
            </span>
          </button>

          <!-- GitHub token (OS keyring) -->
          <div
            class="mb-1 mt-3 border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
            style="border-color: var(--border);"
          >
            GitHub token
          </div>
          <p class="mb-2 text-[10.5px] leading-snug text-[var(--fg3)]">
            Stored in your OS keychain for syncing private repos.
            {ghStored ? "A token is saved." : "No token saved."}
          </p>
          <div class="flex gap-1.5">
            <input
              class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="password"
              autocomplete="off"
              placeholder={ghStored ? "Replace token…" : "ghp_…"}
              bind:value={ghToken}
              disabled={ghBusy}
              onkeydown={(e) => {
                if (e.key === "Enter") saveToken();
              }}
            />
            <button
              class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[11.5px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
              disabled={ghBusy || !ghToken.trim()}
              onclick={saveToken}
            >
              Save
            </button>
          </div>
          {#if ghStored}
            <button
              class="mt-1.5 text-[10.5px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
              disabled={ghBusy}
              onclick={clearToken}
            >
              Remove saved token
            </button>
          {/if}

          <!-- Gmail OAuth client (bring-your-own Google Desktop client; OS keyring) -->
          <div
            class="mb-1 mt-3 border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
            style="border-color: var(--border);"
          >
            Gmail (Google OAuth client)
          </div>
          <p class="mb-2 text-[10.5px] leading-snug text-[var(--fg3)]">
            To pull emails into a brief, create your own Google Cloud OAuth
            <strong class="text-[var(--fg2)]">Desktop</strong> client (Gmail API enabled), then paste its ID + secret
            here. {gmailClientStored ? "A client is saved." : "No client saved."}
          </p>
          <p
            class="mb-2 rounded-md px-2 py-1.5 text-[10px] leading-snug text-[var(--fg2)]"
            style="background: color-mix(in srgb, var(--status-blocked) 12%, transparent);"
          >
            <Icon name="warning" size={11} class="-mt-px mr-0.5" />
            On the OAuth consent screen, set publishing status to
            <strong>“In production”</strong> (unverified is fine for personal use). In
            <strong>Testing</strong>, Google revokes the grant after 7 days and you'd have to reconnect weekly.
          </p>
          <input
            class="mb-1.5 w-full rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            type="text"
            autocomplete="off"
            placeholder={gmailClientStored ? "Replace client ID…" : "…apps.googleusercontent.com"}
            bind:value={gmailClientId}
            disabled={gmailBusy}
          />
          <div class="flex gap-1.5">
            <input
              class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="password"
              autocomplete="off"
              placeholder={gmailClientStored ? "Replace client secret…" : "Client secret (GOCSPX-…)"}
              bind:value={gmailClientSecret}
              disabled={gmailBusy}
              onkeydown={(e) => {
                if (e.key === "Enter") saveGmailClient();
              }}
            />
            <button
              class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[11.5px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
              disabled={gmailBusy || !gmailClientId.trim() || !gmailClientSecret.trim()}
              onclick={saveGmailClient}
            >
              Save
            </button>
          </div>
          {#if gmailClientStored}
            <button
              class="mt-1.5 text-[10.5px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
              disabled={gmailBusy}
              onclick={clearGmailClient}
            >
              Remove saved client
            </button>
          {/if}

          <!-- AI synthesis (Current State + Open Questions) -->
          <div
            class="mb-1 mt-3 border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
            style="border-color: var(--border);"
          >
            AI synthesis
          </div>
          <p class="mb-2 text-[10.5px] leading-snug text-[var(--fg3)]">
            Summarize each brief's Current State &amp; Open Questions from its links and Captures.
          </p>
          <select
            class="mb-2 w-full rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            value={llmProvider}
            onchange={(e) => onProviderChange(e.currentTarget.value)}
          >
            <option value="">Off</option>
            <option value="ollama">Ollama (local)</option>
            <option value="anthropic">Anthropic (cloud)</option>
          </select>

          {#if llmProvider === "ollama"}
            <input
              class="mb-1.5 w-full rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="text"
              autocomplete="off"
              placeholder={OLLAMA_DEFAULT_URL}
              bind:value={ollamaUrl}
              onblur={() => {
                saveLlmSettings();
                fetchOllamaModels();
              }}
            />
            <select
              class="w-full rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              bind:value={ollamaModel}
              onchange={saveLlmSettings}
            >
              {#if ollamaModels.length === 0}
                <option value="">No models found</option>
              {:else}
                {#each ollamaModels as m (m)}
                  <option value={m}>{m}</option>
                {/each}
              {/if}
            </select>
            {#if ollamaError}
              <p class="mt-1.5 text-[10.5px] leading-snug text-[var(--status-blocked)]">{ollamaError}</p>
            {/if}
          {:else if llmProvider === "anthropic"}
            <input
              class="mb-1.5 w-full rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="text"
              autocomplete="off"
              placeholder="claude-haiku-4-5-20251001"
              bind:value={anthropicModel}
              onblur={saveLlmSettings}
            />
            <p class="mb-2 text-[10.5px] leading-snug text-[var(--fg3)]">
              API key stored in your OS keychain.
              {anthropicStored ? "A key is saved." : "No key saved."}
            </p>
            <div class="flex gap-1.5">
              <input
                class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[11.5px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="password"
                autocomplete="off"
                placeholder={anthropicStored ? "Replace key…" : "sk-ant-…"}
                bind:value={anthropicKey}
                disabled={llmBusy}
                onkeydown={(e) => {
                  if (e.key === "Enter") saveAnthropicKey();
                }}
              />
              <button
                class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[11.5px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
                disabled={llmBusy || !anthropicKey.trim()}
                onclick={saveAnthropicKey}
              >
                Save
              </button>
            </div>
            {#if anthropicStored}
              <button
                class="mt-1.5 text-[10.5px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
                disabled={llmBusy}
                onclick={clearAnthropicKey}
              >
                Remove saved key
              </button>
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  </header>

  <!-- Search -->
  {#if projects.briefs.length > 0}
    <div class="px-3 pb-[6px] pt-[10px]">
      <div
        class="flex h-[34px] items-center gap-2 rounded-[9px] border px-[10px] text-[12.5px] transition-[border-color,box-shadow] focus-within:border-[var(--accent)] focus-within:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_18%,transparent)]"
        style="background: var(--input-bg); border-color: var(--border);"
      >
        <Icon name="search" size={15} class="text-[var(--fg3)]" />
        <input
          bind:this={searchEl}
          class="min-w-0 flex-1 bg-transparent text-[var(--fg)] outline-none placeholder:text-[var(--fg3)]"
          placeholder="Search projects…"
          type="search"
          bind:value={projects.query}
          onkeydown={(e) => {
            if (e.key === "Escape") projects.query = "";
          }}
        />
        <span
          class="rounded-[5px] border px-[5px] py-px text-[10px] text-[var(--fg3)]"
          style="border-color: var(--border); background: var(--bg);"
        >⌘F</span>
      </div>
    </div>

    <!-- Status filter chips -->
    {#if projects.statuses.length > 1}
      <div class="flex flex-wrap gap-[5px] px-3 pb-2 pt-1">
        {#each projects.statuses as status (status)}
          <button
            class="rounded-full px-[9px] py-[2px] text-[11px] font-medium capitalize transition-colors {projects.statusFilter ===
            status
              ? 'bg-[var(--accent)] text-white'
              : 'bg-[var(--chip-bg)] text-[var(--fg2)] hover:text-[var(--fg)]'}"
            onclick={() => toggleStatus(status)}
          >
            {status}
          </button>
        {/each}
      </div>
    {/if}
  {/if}

  <!-- Project list -->
  <nav class="scroll-thin flex-1 overflow-y-auto px-[10px] pb-[10px] pt-1">
    {#if projects.loading && projects.briefs.length === 0}
      <p class="px-3 py-3 text-[12.5px] text-[var(--fg3)]">Loading…</p>
    {:else if projects.error}
      <p class="px-3 py-3 text-[12.5px] text-[var(--status-blocked)]">{projects.error}</p>
    {:else if projects.briefs.length === 0}
      <p class="px-3 py-3 text-[12.5px] text-[var(--fg3)]">No briefs yet. Create one below.</p>
    {:else if projects.filtered.length === 0}
      <p class="px-3 py-3 text-[12.5px] text-[var(--fg3)]">No matching projects.</p>

    {:else if settings.sidebarStyle === "rows"}
      <!-- a) ROWS — flat, detailed -->
      {#each projects.filtered as brief (brief.path)}
        <button
          class="mb-px w-full rounded-[9px] px-[11px] py-2 text-left transition-colors {brief.path ===
          projects.selectedPath
            ? 'bg-[var(--sel-bg)] shadow-[0_0_0_1px_var(--sel-ring),0_1px_3px_rgba(15,30,60,0.06)] dark:rounded-[0_9px_9px_0] dark:shadow-[inset_2px_0_0_var(--accent)]'
            : 'hover:bg-[var(--hover)]'}"
          onclick={() => projects.select(brief.path)}
        >
          <div class="flex items-center gap-2">
            <span class="flex-1 truncate text-[13px] font-medium text-[var(--fg)]">{brief.name}</span>
            <StatusPill status={brief.status} />
          </div>
          {#if brief.description}
            <p class="mt-[2px] truncate text-[11.5px] text-[var(--fg3)]">{brief.description}</p>
          {/if}
          <p class="mt-[3px] text-[10.5px] text-[var(--fg3)]">{relativeTime(brief.lastOpened)}</p>
        </button>
      {/each}

    {:else if settings.sidebarStyle === "compact"}
      <!-- b) COMPACT — grouped, dense single-line rows -->
      {#each groups as group (group.status)}
        <div
          class="flex items-center gap-[7px] px-2 pb-1 pt-[11px] text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
        >
          {groupLabel(group.status)}
          <span class="ml-auto tabular-nums">{group.items.length}</span>
        </div>
        {#each group.items as brief (brief.path)}
          <button
            class="flex h-[30px] w-full items-center gap-[9px] rounded-[7px] px-[9px] text-left {brief.path ===
            projects.selectedPath
              ? 'bg-[var(--sel-bg)] rounded-[0_7px_7px_0] shadow-[inset_2px_0_0_var(--accent)]'
              : 'hover:bg-[var(--hover)]'}"
            onclick={() => projects.select(brief.path)}
          >
            <span class="sdot h-[7px] w-[7px]" style="--sc: {statusColor(brief.status)};"></span>
            <span
              class="flex-1 truncate text-[12.5px] text-[var(--fg)] {brief.path ===
              projects.selectedPath
                ? 'font-[550]'
                : 'font-[450]'}"
            >{brief.name}</span>
            <span class="text-[10px] tabular-nums text-[var(--fg3)]">{relativeTime(brief.lastOpened)}</span>
          </button>
        {/each}
      {/each}

    {:else}
      <!-- c) ROCKS — grouped, expressive priority-tinted cards -->
      {#each groups as group (group.status)}
        <div
          class="flex items-center gap-[6px] px-[6px] pb-[7px] pt-[13px] text-[10px] font-bold uppercase tracking-[0.07em] text-[var(--fg3)]"
        >
          <span class="sdot h-[7px] w-[7px]" style="--sc: {statusColor(group.status)};"></span>
          {groupLabel(group.status)}
          <span class="ml-auto tabular-nums">{group.items.length}</span>
        </div>
        {#each group.items as brief (brief.path)}
          {@const pc = statusColor(brief.status)}
          {@const sel = brief.path === projects.selectedPath}
          <button
            class="mb-[7px] block w-full cursor-pointer rounded-[13px] border-solid bg-[var(--bg)] px-[11px] py-[9px] pl-[13px] text-left transition-[transform,box-shadow] hover:-translate-y-px dark:bg-white/[0.02]"
            style="border-width: {sel ? '2px' : '1.5px'}; border-color: color-mix(in srgb, {pc} {sel
              ? '85%'
              : '38%'}, transparent); {sel
              ? `box-shadow: 0 3px 12px color-mix(in srgb, ${pc} 28%, transparent);`
              : ''}"
            onclick={() => projects.select(brief.path)}
          >
            <div class="flex items-center gap-2">
              <span
                class="grid h-[22px] w-[22px] flex-shrink-0 place-items-center"
                style="border-radius: 8px 11px 9px 12px; background: color-mix(in srgb, {pc} {settings.dark
                  ? '22%'
                  : '16%'}, transparent);"
              >
                <BrandMark size={13} color={pc} />
              </span>
              <span class="flex-1 truncate text-[12.5px] font-semibold text-[var(--fg)]">{brief.name}</span>
              <span class="text-[10px] text-[var(--fg3)]">{relativeTime(brief.lastOpened)}</span>
            </div>
            {#if brief.description}
              <div
                class="mt-1 overflow-hidden text-[11px] leading-[1.45] text-[var(--fg3)]"
                style="display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;"
              >{brief.description}</div>
            {/if}
          </button>
        {/each}
      {/each}
    {/if}
  </nav>

  <!-- Footer -->
  <footer class="flex flex-col gap-2 border-t px-3 py-[10px]" style="border-color: var(--border);">
    {#if creating}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="h-8 rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none"
        style="border-color: var(--accent); background: var(--input-bg);"
        placeholder="New project name…"
        autofocus
        bind:value={newName}
        onkeydown={(e) => {
          if (e.key === "Enter") submitNew();
          if (e.key === "Escape") {
            creating = false;
            newName = "";
          }
        }}
        onblur={submitNew}
      />
    {:else}
      <button
        class="flex h-8 items-center justify-center gap-[6px] rounded-[9px] bg-[var(--accent)] text-[12.5px] font-semibold text-white transition-[filter] hover:brightness-[1.06]"
        onclick={startNew}
      >
        <Icon name="add" size={16} /> New project
      </button>
    {/if}

    <div class="flex items-center justify-between gap-2 px-[2px] text-[10.5px] text-[var(--fg3)]">
      <span class="truncate" title={projects.briefsDir}>{projects.briefsDir || "…"}</span>
      <button class="shrink-0 text-[var(--accent)] hover:underline" onclick={changeFolder}>Change</button>
    </div>
  </footer>
</aside>

<BriefingModal open={briefingOpen} onclose={() => (briefingOpen = false)} />
