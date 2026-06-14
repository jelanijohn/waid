<script lang="ts">
  // The "View & appearance" menu — appearance tweaks plus token / LLM / Gmail
  // management. Lives in the unified toolbar (Titlebar.svelte), anchored to its
  // own `tune` button. Self-contained: owns the popover open state and every
  // form's local state + handlers (moved out of Sidebar's old brand header).
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import {
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
  import {
    settings,
    ACCENTS,
    type SidebarStyle,
    type Density,
    type BriefLayout,
  } from "$lib/stores/settings.svelte";
  import Icon from "./Icon.svelte";
  import CredentialHelp from "./CredentialHelp.svelte";

  let settingsOpen = $state(false);

  // GitHub token (OS keyring).
  let ghToken = $state("");
  let ghStored = $state(false);
  let ghBusy = $state(false);

  // Gmail OAuth client (bring-your-own Desktop client; OS keyring). Both id +
  // secret are required before Connect works.
  let gmailClientId = $state("");
  let gmailClientSecret = $state("");
  let gmailClientStored = $state(false);
  let gmailBusy = $state(false);

  // LLM synthesis settings. Provider "" means disabled.
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

  const SIDEBAR_STYLES: { value: SidebarStyle; label: string }[] = [
    { value: "rows", label: "Rows" },
    { value: "compact", label: "Compact" },
    { value: "rocks", label: "Rocks" },
  ];
  const BRIEF_LAYOUTS: { value: BriefLayout; label: string }[] = [
    { value: "two-col", label: "Two-column" },
    { value: "body", label: "Body first" },
    { value: "quiet", label: "Quiet top" },
  ];
  const DENSITIES: { value: Density; label: string }[] = [
    { value: "comfortable", label: "Comfortable" },
    { value: "compact", label: "Compact" },
  ];
</script>

<div class="relative flex items-center">
  <button class="btn-icon" title="View & appearance" aria-label="View & appearance" onclick={toggleSettings}>
    <Icon name="tune" size={16} />
  </button>

  {#if settingsOpen}
    <!-- click-away layer -->
    <button
      class="fixed inset-0 z-40 cursor-default"
      aria-label="Close settings"
      onclick={() => (settingsOpen = false)}
    ></button>
    <div
      class="anim-pop absolute right-0 top-[36px] z-50 max-h-[calc(100vh-72px)] w-[232px] overflow-y-auto overscroll-contain rounded-[14px] border p-3"
      style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
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

      <!-- Brief layout -->
      <div class="mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
        Brief layout
      </div>
      <div class="mb-3 flex rounded-lg border p-[2px]" style="border-color: var(--border);">
        {#each BRIEF_LAYOUTS as opt (opt.value)}
          <button
            class="flex-1 rounded-md px-2 py-1 text-[11.5px] font-medium transition-colors {settings.briefLayout ===
            opt.value
              ? 'bg-[var(--accent)] text-white'
              : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
            onclick={() => settings.setBriefLayout(opt.value)}
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
        class="mb-1 mt-3 flex items-center justify-between border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
        style="border-color: var(--border);"
      >
        GitHub token
        <CredentialHelp topic="github" />
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
        class="mb-1 mt-3 flex items-center justify-between border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
        style="border-color: var(--border);"
      >
        Gmail (Google OAuth client)
        <CredentialHelp topic="gmail-client" />
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
        <div class="mb-2 flex items-start justify-between gap-2">
          <p class="text-[10.5px] leading-snug text-[var(--fg3)]">
            API key stored in your OS keychain.
            {anthropicStored ? "A key is saved." : "No key saved."}
          </p>
          <CredentialHelp topic="anthropic" />
        </div>
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
