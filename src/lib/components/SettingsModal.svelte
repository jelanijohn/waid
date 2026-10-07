<script lang="ts">
  // Settings window — a centered modal dialog with a section nav on the left
  // (Appearance / Sync / Gmail / AI synthesis). Replaces AppMenu's old narrow
  // anchored popover; AppMenu now just owns the toolbar button that opens this.
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import {
    setSecret,
    deleteSecret,
    hasSecret,
    SECRET_GITHUB_TOKEN,
    SECRET_ANTHROPIC_API_KEY,
    openaiKeySecret,
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
    type WidgetLeaf,
  } from "$lib/stores/settings.svelte";
  import Icon from "./Icon.svelte";
  import CredentialHelp from "./CredentialHelp.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  // Move focus into the dialog on open — the Escape handler lives on the
  // dialog's onkeydown, so it only works once focus is inside.
  let dialogEl = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (open && dialogEl) dialogEl.focus();
  });

  type Section = "appearance" | "sync" | "gmail" | "ai";
  let section = $state<Section>("appearance");

  const THEMES: { value: boolean; label: string; icon: string }[] = [
    { value: false, label: "Light", icon: "light_mode" },
    { value: true, label: "Dark", icon: "dark_mode" },
  ];

  const SECTIONS: { value: Section; label: string; icon: string }[] = [
    { value: "appearance", label: "Appearance", icon: "palette" },
    { value: "sync", label: "Sync & GitHub", icon: "sync" },
    { value: "gmail", label: "Gmail", icon: "mail" },
    { value: "ai", label: "AI synthesis", icon: "auto_awesome" },
  ];

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
  let llmProvider = $state<"" | "ollama" | "anthropic" | "openai">("");
  let ollamaUrl = $state("");
  let ollamaModel = $state("");
  let ollamaModels = $state<string[]>([]);
  let ollamaError = $state("");
  let anthropicModel = $state("");
  let anthropicKey = $state("");
  let anthropicStored = $state(false);
  let openaiUrl = $state("");
  let openaiModel = $state("");
  let openaiKey = $state("");
  /** Keyring name of the key for the *current* URL's origin. */
  let openaiSecretName = $derived(openaiUrl.trim() ? openaiKeySecret(openaiUrl) : "");
  /** The secret name a keyring lookup last confirmed a key exists under, or
   *  null. "Stored" is derived by comparing it against `openaiSecretName`, so
   *  editing the URL invalidates the status instantly instead of showing (or
   *  letting Remove clear) the previous origin's key. */
  let openaiStoredFor = $state<string | null>(null);
  let openaiStored = $derived(openaiSecretName !== "" && openaiStoredFor === openaiSecretName);
  /** Plain `http://` to a host that isn't loopback: brief evidence (and any
   *  saved key) would travel unencrypted. Informational only — never blocks,
   *  since LAN / WSL2-host servers are a legitimate plain-HTTP case. */
  let openaiInsecureRemote = $derived.by(() => {
    try {
      const u = new URL(openaiUrl.trim());
      if (u.protocol !== "http:") return false;
      const host = u.hostname.replace(/^\[|\]$/g, "").toLowerCase();
      return !(
        host === "localhost" ||
        host.endsWith(".localhost") ||
        host === "::1" ||
        host === "0.0.0.0" ||
        // Numeric 127/8 only — a DNS name like 127.example.com is remote.
        /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host)
      );
    } catch {
      return false;
    }
  });
  let llmBusy = $state(false);

  const OLLAMA_DEFAULT_URL = "http://localhost:11434";

  // Reset transient input state and refresh stored-credential statuses each
  // time the window opens.
  let wasOpen = $state(false);
  $effect(() => {
    if (open && !wasOpen) {
      ghToken = "";
      gmailClientId = "";
      gmailClientSecret = "";
      refreshTokenStatus();
      refreshGmailClientStatus();
      loadLlmSettings();
    }
    wasOpen = open;
  });

  async function resetOrder() {
    try {
      await projects.resetOrder();
      toasts.success("Project order reset");
    } catch (e) {
      toasts.error(`Could not reset order: ${e}`);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  async function refreshGmailClientStatus() {
    // Connect needs both halves of the OAuth client, so only report "saved"
    // when both are present (a failed save can leave just the id behind).
    try {
      gmailClientStored =
        (await hasSecret(SECRET_GMAIL_CLIENT_ID)) &&
        (await hasSecret(SECRET_GMAIL_CLIENT_SECRET));
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
      llmProvider = (s.llmProvider as "" | "ollama" | "anthropic" | "openai") ?? "";
      ollamaUrl = s.ollamaUrl ?? OLLAMA_DEFAULT_URL;
      ollamaModel = s.ollamaModel ?? "";
      anthropicModel = s.anthropicModel ?? "";
      openaiUrl = s.openaiUrl ?? "";
      openaiModel = s.openaiModel ?? "";
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
    await refreshOpenaiKeyStatus();
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
        openaiUrl: openaiUrl.trim() || null,
        openaiModel: openaiModel.trim() || null,
      });
      await projects.refreshLlmProvider();
    } catch (e) {
      toasts.error(`Could not save AI settings: ${e}`);
    }
  }

  async function onProviderChange(value: string) {
    llmProvider = value as "" | "ollama" | "anthropic" | "openai";
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

  /** The key is scoped to the endpoint's origin, so re-check whenever the URL
   *  changes (a host switch shows "No key saved" for the new host). */
  async function refreshOpenaiKeyStatus() {
    openaiKey = "";
    const name = openaiSecretName;
    if (!name) return;
    let stored = false;
    try {
      stored = await hasSecret(name);
    } catch {
      stored = false;
    }
    // Ignore a lookup that resolved after the URL moved on (out-of-order edits).
    if (name !== openaiSecretName) return;
    openaiStoredFor = stored ? name : null;
  }

  async function saveOpenaiKey() {
    const value = openaiKey.trim();
    const name = openaiSecretName;
    if (!value || !name || llmBusy) return;
    llmBusy = true;
    try {
      await setSecret(name, value);
      openaiKey = "";
      openaiStoredFor = name;
      toasts.success("Endpoint API key saved to keychain");
    } catch (e) {
      toasts.error(`Could not save key: ${e}`);
    } finally {
      llmBusy = false;
    }
  }

  async function clearOpenaiKey() {
    // Only ever removes the key the status was confirmed for.
    const name = openaiStoredFor;
    if (llmBusy || !name || name !== openaiSecretName) return;
    llmBusy = true;
    try {
      await deleteSecret(name);
      openaiStoredFor = null;
      toasts.success("Endpoint API key removed");
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
  const WIDGET_LEAVES: { value: WidgetLeaf; label: string }[] = [
    { value: "accordion", label: "Accordion" },
    { value: "side", label: "Side leaf" },
  ];
  const DENSITIES: { value: Density; label: string }[] = [
    { value: "comfortable", label: "Comfortable" },
    { value: "compact", label: "Compact" },
  ];
</script>

{#if open}
  <div
    class="anim-fade fixed inset-[14px] z-50 flex items-center justify-center overflow-hidden rounded-[12px]"
    style="background: color-mix(in srgb, var(--fg) 22%, transparent); backdrop-filter: blur(2px);"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="anim-pop flex h-[min(78vh,560px)] w-[min(94%,640px)] flex-col overflow-hidden rounded-[16px] border"
      style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      tabindex="-1"
      bind:this={dialogEl}
      onkeydown={onKeydown}
    >
      <div class="flex items-center justify-between gap-3 border-b px-4 py-3" style="border-color: var(--border);">
        <h3 class="flex items-center gap-[7px] text-[14px] font-semibold text-[var(--fg)]">
          <Icon name="tune" size={16} class="text-[var(--accent)]" /> Settings
        </h3>
        <button
          class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
          aria-label="Close"
          onclick={onclose}
        >
          <Icon name="close" size={16} />
        </button>
      </div>

      <div class="flex min-h-0 flex-1">
        <!-- Section nav -->
        <nav
          class="flex w-[168px] shrink-0 flex-col gap-0.5 border-r p-2"
          style="border-color: var(--border);"
        >
          {#each SECTIONS as s (s.value)}
            <button
              class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] font-medium transition-colors {section ===
              s.value
                ? 'bg-[var(--hover)] text-[var(--fg)]'
                : 'text-[var(--fg2)] hover:bg-[var(--hover)] hover:text-[var(--fg)]'}"
              onclick={() => (section = s.value)}
            >
              <Icon
                name={s.icon}
                size={15}
                class={section === s.value ? "text-[var(--accent)]" : "text-[var(--fg3)]"}
              />
              {s.label}
            </button>
          {/each}
        </nav>

        <!-- Section content -->
        <div class="scroll-thin min-w-0 flex-1 overflow-y-auto px-5 py-4">
          {#if section === "appearance"}
            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Theme
            </div>
            <div class="mb-4 flex max-w-[340px] rounded-lg border p-[2px]" style="border-color: var(--border);">
              {#each THEMES as opt (opt.value)}
                <button
                  class="flex flex-1 items-center justify-center gap-1.5 rounded-md px-2 py-1 text-[12px] font-medium transition-colors {settings.dark ===
                  opt.value
                    ? 'bg-[var(--accent)] text-white'
                    : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                  aria-pressed={settings.dark === opt.value}
                  onclick={() => settings.setDark(opt.value)}
                >
                  <Icon name={opt.icon} size={14} />
                  {opt.label}
                </button>
              {/each}
            </div>

            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Project cards
            </div>
            <div class="mb-4 flex max-w-[340px] rounded-lg border p-[2px]" style="border-color: var(--border);">
              {#each SIDEBAR_STYLES as opt (opt.value)}
                <button
                  class="flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors {settings.sidebarStyle ===
                  opt.value
                    ? 'bg-[var(--accent)] text-white'
                    : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                  onclick={() => settings.setSidebarStyle(opt.value)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>

            <button
              class="mb-4 flex w-full max-w-[340px] items-center justify-between gap-2 text-left"
              onclick={() => settings.setSidebarZebra(!settings.sidebarZebra)}
              aria-pressed={settings.sidebarZebra}
            >
              <span class="text-[12px] text-[var(--fg2)]">
                Alternate row shading
                <span class="block text-[10.5px] text-[var(--fg3)]">Rows and Compact lists only</span>
              </span>
              <span
                class="relative h-[18px] w-[30px] shrink-0 rounded-full transition-colors"
                style="background: {settings.sidebarZebra ? 'var(--accent)' : 'var(--border)'};"
              >
                <span
                  class="absolute top-[2px] h-[14px] w-[14px] rounded-full bg-white transition-[left]"
                  style="left: {settings.sidebarZebra ? '14px' : '2px'};"
                ></span>
              </span>
            </button>

            <div class="mb-4 flex w-full max-w-[340px] items-center justify-between gap-2">
              <span class="text-[12px] text-[var(--fg2)]">
                Project order
                <span class="block text-[10.5px] text-[var(--fg3)]">Drag to reorder, or Alt+↑/↓ on the selected project</span>
              </span>
              <button
                class="shrink-0 rounded-[7px] border px-[9px] py-[4px] text-[11px] font-medium text-[var(--fg2)] transition-colors hover:text-[var(--fg)]"
                style="border-color: var(--border);"
                title="Forget the manual order and sort by most recently opened"
                onclick={resetOrder}
              >
                Reset to recent
              </button>
            </div>

            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Brief layout
            </div>
            <div class="mb-4 flex max-w-[340px] rounded-lg border p-[2px]" style="border-color: var(--border);">
              {#each BRIEF_LAYOUTS as opt (opt.value)}
                <button
                  class="flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors {settings.briefLayout ===
                  opt.value
                    ? 'bg-[var(--accent)] text-white'
                    : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                  onclick={() => settings.setBriefLayout(opt.value)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>

            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Widget detail
            </div>
            <div class="mb-4 flex max-w-[340px] rounded-lg border p-[2px]" style="border-color: var(--border);">
              {#each WIDGET_LEAVES as opt (opt.value)}
                <button
                  class="flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors {settings.widgetLeaf ===
                  opt.value
                    ? 'bg-[var(--accent)] text-white'
                    : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                  onclick={() => settings.setWidgetLeaf(opt.value)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>

            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Accent
            </div>
            <div class="mb-4 flex gap-2">
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

            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Density
            </div>
            <div class="flex max-w-[340px] rounded-lg border p-[2px]" style="border-color: var(--border);">
              {#each DENSITIES as opt (opt.value)}
                <button
                  class="flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors {settings.density ===
                  opt.value
                    ? 'bg-[var(--accent)] text-white'
                    : 'text-[var(--fg2)] hover:text-[var(--fg)]'}"
                  onclick={() => settings.setDensity(opt.value)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          {:else if section === "sync"}
            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              Sync
            </div>
            <button
              class="flex w-full max-w-[340px] items-center justify-between gap-2 text-left"
              onclick={() => settings.setAutoSyncOnOpen(!settings.autoSyncOnOpen)}
            >
              <span class="text-[12px] text-[var(--fg2)]">Auto-sync on open</span>
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

            <div
              class="mb-1.5 mt-4 flex max-w-[340px] items-center justify-between border-t pt-4 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
              style="border-color: var(--border);"
            >
              GitHub token
              <CredentialHelp topic="github" />
            </div>
            <p class="mb-2 max-w-[340px] text-[11px] leading-snug text-[var(--fg3)]">
              Stored in your OS keychain for syncing private repos.
              {ghStored ? "A token is saved." : "No token saved."}
            </p>
            <div class="flex max-w-[340px] gap-1.5">
              <input
                class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
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
                class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[12px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
                disabled={ghBusy || !ghToken.trim()}
                onclick={saveToken}
              >
                Save
              </button>
            </div>
            {#if ghStored}
              <button
                class="mt-1.5 text-[11px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
                disabled={ghBusy}
                onclick={clearToken}
              >
                Remove saved token
              </button>
            {/if}
          {:else if section === "gmail"}
            <div
              class="mb-1.5 flex max-w-[400px] items-center justify-between text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
            >
              Gmail (Google OAuth client)
              <CredentialHelp topic="gmail-client" />
            </div>
            <p class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--fg3)]">
              To pull emails into a brief, create your own Google Cloud OAuth
              <strong class="text-[var(--fg2)]">Desktop</strong> client (Gmail API enabled), then paste its ID +
              secret here. {gmailClientStored ? "A client is saved." : "No client saved."}
            </p>
            <p
              class="mb-3 max-w-[400px] rounded-md px-2 py-1.5 text-[10.5px] leading-snug text-[var(--fg2)]"
              style="background: color-mix(in srgb, var(--status-blocked) 12%, transparent);"
            >
              <Icon name="warning" size={11} class="-mt-px mr-0.5" />
              On the OAuth consent screen, set publishing status to
              <strong>“In production”</strong> (unverified is fine for personal use). In
              <strong>Testing</strong>, Google revokes the grant after 7 days and you'd have to reconnect weekly.
            </p>
            <input
              class="mb-1.5 w-full max-w-[400px] rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="text"
              autocomplete="off"
              placeholder={gmailClientStored ? "Replace client ID…" : "…apps.googleusercontent.com"}
              bind:value={gmailClientId}
              disabled={gmailBusy}
            />
            <div class="flex max-w-[400px] gap-1.5">
              <input
                class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
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
                class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[12px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
                disabled={gmailBusy || !gmailClientId.trim() || !gmailClientSecret.trim()}
                onclick={saveGmailClient}
              >
                Save
              </button>
            </div>
            {#if gmailClientStored}
              <button
                class="mt-1.5 text-[11px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
                disabled={gmailBusy}
                onclick={clearGmailClient}
              >
                Remove saved client
              </button>
            {/if}
          {:else if section === "ai"}
            <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
              AI synthesis
            </div>
            <p class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--fg3)]">
              Summarize each brief's Current State &amp; Open Questions from its links and Captures.
            </p>
            <select
              class="themed-select mb-3 w-full max-w-[400px] rounded-md py-1 text-[12px]"
              aria-label="AI synthesis provider"
              value={llmProvider}
              onchange={(e) => onProviderChange(e.currentTarget.value)}
            >
              <option value="">Off</option>
              <option value="ollama">Ollama (local)</option>
              <option value="anthropic">Anthropic (cloud)</option>
              <option value="openai">OpenAI-compatible (custom)</option>
            </select>

            {#if llmProvider === "ollama"}
              <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
                Ollama
              </div>
              <input
                class="mb-1.5 w-full max-w-[400px] rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
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
                class="themed-select w-full max-w-[400px] rounded-md py-1 text-[12px]"
                aria-label="Ollama model"
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
                <p class="mt-1.5 max-w-[400px] text-[11px] leading-snug text-[var(--status-blocked)]">
                  {ollamaError}
                </p>
              {/if}
            {:else if llmProvider === "anthropic"}
              <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
                Anthropic
              </div>
              <input
                class="mb-1.5 w-full max-w-[400px] rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                autocomplete="off"
                placeholder="claude-haiku-4-5-20251001"
                bind:value={anthropicModel}
                onblur={saveLlmSettings}
              />
              <div class="mb-2 flex max-w-[400px] items-start justify-between gap-2">
                <p class="text-[11px] leading-snug text-[var(--fg3)]">
                  API key stored in your OS keychain.
                  {anthropicStored ? "A key is saved." : "No key saved."}
                </p>
                <CredentialHelp topic="anthropic" />
              </div>
              <div class="flex max-w-[400px] gap-1.5">
                <input
                  class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
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
                  class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[12px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
                  disabled={llmBusy || !anthropicKey.trim()}
                  onclick={saveAnthropicKey}
                >
                  Save
                </button>
              </div>
              {#if anthropicStored}
                <button
                  class="mt-1.5 text-[11px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
                  disabled={llmBusy}
                  onclick={clearAnthropicKey}
                >
                  Remove saved key
                </button>
              {/if}
            {:else if llmProvider === "openai"}
              <div
                class="mb-1.5 flex max-w-[400px] items-center justify-between text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
              >
                OpenAI-compatible
                <CredentialHelp topic="openai-compat" />
              </div>
              <input
                class="mb-1.5 w-full max-w-[400px] rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                autocomplete="off"
                placeholder="https://openrouter.ai/api/v1 · http://localhost:1234/v1"
                aria-label="Endpoint URL"
                bind:value={openaiUrl}
                onblur={() => {
                  saveLlmSettings();
                  refreshOpenaiKeyStatus();
                }}
              />
              <p class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--fg3)]">
                Any server speaking OpenAI's chat-completions API. Include the <code>/v1</code> if
                your server uses one; WAID appends <code>/chat/completions</code>. localhost = fully
                private; a remote URL sends brief content to that provider.
              </p>
              {#if openaiInsecureRemote}
                <p
                  class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--status-paused)]"
                  role="status"
                >
                  Plain <code>http://</code> to a non-local host: brief content and any saved API key
                  are sent unencrypted. Prefer <code>https://</code> unless this server is on a
                  network you trust.
                </p>
              {/if}
              <input
                class="mb-1.5 w-full max-w-[400px] rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                autocomplete="off"
                placeholder="e.g. mistralai/mistral-small · llama-3.1-8b"
                aria-label="Model id"
                bind:value={openaiModel}
                onblur={saveLlmSettings}
              />
              <p class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--fg3)]">
                Model id as your provider documents it — pick one; synthesis won't run without it.
              </p>
              <p class="mb-2 max-w-[400px] text-[11px] leading-snug text-[var(--fg3)]">
                API key: optional — cloud providers need one; local servers usually don't. Stored in
                your OS keychain for this endpoint's host only.
                {openaiUrl.trim()
                  ? openaiStored
                    ? "A key is saved for this host."
                    : "No key saved for this host."
                  : "Enter the endpoint URL first."}
              </p>
              <div class="flex max-w-[400px] gap-1.5">
                <input
                  class="min-w-0 flex-1 rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none transition-colors focus:border-[var(--accent)]"
                  style="background: var(--input-bg); border-color: var(--border);"
                  type="password"
                  autocomplete="off"
                  placeholder={openaiStored ? "Replace key…" : "API key (optional)"}
                  aria-label="API key"
                  bind:value={openaiKey}
                  disabled={llmBusy || !openaiUrl.trim()}
                  onkeydown={(e) => {
                    if (e.key === "Enter") saveOpenaiKey();
                  }}
                />
                <button
                  class="shrink-0 rounded-md bg-[var(--accent)] px-2.5 py-1 text-[12px] font-semibold text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
                  disabled={llmBusy || !openaiKey.trim() || !openaiUrl.trim()}
                  onclick={saveOpenaiKey}
                >
                  Save
                </button>
              </div>
              {#if openaiStored}
                <button
                  class="mt-1.5 text-[11px] text-[var(--fg3)] transition-colors hover:text-[var(--fg)] hover:underline disabled:opacity-50"
                  disabled={llmBusy}
                  onclick={clearOpenaiKey}
                >
                  Remove saved key
                </button>
              {/if}
            {/if}
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
