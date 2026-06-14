<script lang="ts">
  // Unified toolbar — the app's own window chrome, drawn once across the full
  // window width. Replaces the stock OS title bar and the sidebar's old brand
  // header. Window controls flip sides by platform: macOS keeps its native
  // traffic lights (Overlay title-bar style, set in tauri.conf.json); Windows &
  // Linux/WSL turn decorations off (lib.rs) and we draw our own caption buttons.
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { projects } from "$lib/stores/projects.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { statusColor } from "$lib/status";
  import Icon from "./Icon.svelte";
  import BrandMark from "./BrandMark.svelte";
  import AppMenu from "./AppMenu.svelte";
  import BriefingModal from "./BriefingModal.svelte";

  /** Platform decides which side the window controls live on. We sniff the
   *  webview's userAgent (no extra plugin): WebKitGTK on Linux/WSL → "Linux",
   *  WebView2 → "Windows", WKWebView → "Macintosh". */
  function detectPlatform(): "macos" | "windows" | "linux" {
    const ua = navigator.userAgent;
    if (/Mac OS X|Macintosh/.test(ua)) return "macos";
    if (/Windows/.test(ua)) return "windows";
    return "linux";
  }

  let plat = $state<"macos" | "windows" | "linux">(detectPlatform());
  let maximized = $state(false);
  let briefingOpen = $state(false);
  let syncing = $state(false);
  const win = getCurrentWindow();

  const isMac = $derived(plat === "macos");
  // Current project drives the breadcrumb; no selection → just "WAID".
  let current = $derived(projects.selected);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        maximized = await win.isMaximized();
        unlisten = await win.onResized(async () => {
          maximized = await win.isMaximized();
        });
      } catch {
        // Window APIs unavailable (e.g. plain `vite dev` in a browser) — the
        // caption buttons simply won't do anything there.
      }
    })();
    return () => unlisten?.();
  });

  async function syncAll() {
    if (syncing) return;
    syncing = true;
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
      syncing = false;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="titlebar"
  data-platform={plat}
  data-tauri-drag-region
  ondblclick={() => !isMac && win.toggleMaximize()}
>
  <!-- macOS: reserve space for the native traffic lights + a divider. -->
  {#if isMac}
    <span class="mac-inset" data-tauri-drag-region></span>
    <span class="vdiv"></span>
  {/if}

  <span class="brandchip"><BrandMark size={14} /></span>
  <span class="wordmark" data-tauri-drag-region>WAID</span>

  {#if current}
    <span class="chev"><Icon name="chevron_right" size={16} /></span>
    <span class="sdot" style="background: {statusColor(current.status)};"></span>
    <span class="crumb" data-tauri-drag-region>{current.name}</span>
  {/if}

  <div class="actions">
    {#if projects.llmProvider}
      <button class="btn-icon" title="Morning briefing across all projects" aria-label="Morning briefing" onclick={() => (briefingOpen = true)}>
        <Icon name="wb_sunny" size={16} />
      </button>
    {/if}
    <button class="btn-icon" title="Sync all projects" aria-label="Sync all projects" onclick={syncAll} disabled={syncing}>
      <Icon name="sync" size={16} class={syncing ? "spin" : ""} />
    </button>
    <AppMenu />
    <button class="btn-icon" title="Toggle theme" aria-label="Toggle theme" onclick={() => settings.toggleDark()}>
      <Icon name={settings.dark ? "light_mode" : "dark_mode"} size={16} />
    </button>
  </div>

  <!-- Windows / Linux: custom caption buttons, flush right. -->
  {#if !isMac}
    <div class="caption">
      <button class="cap" title="Minimize" onclick={() => win.minimize()} aria-label="Minimize">
        <svg viewBox="0 0 10 10"><line x1="0" y1="5.5" x2="10" y2="5.5" /></svg>
      </button>
      <button class="cap" title={maximized ? "Restore" : "Maximize"} onclick={() => win.toggleMaximize()} aria-label={maximized ? "Restore" : "Maximize"}>
        {#if maximized}
          <svg viewBox="0 0 10 10"><rect x="0.5" y="2.5" width="7" height="7" /><path d="M2.5 2.5 V0.5 H9.5 V7.5 H7.5" /></svg>
        {:else}
          <svg viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" /></svg>
        {/if}
      </button>
      <button class="cap close" title="Close" onclick={() => win.close()} aria-label="Close">
        <svg viewBox="0 0 10 10"><path d="M0.7 0.7 L9.3 9.3 M9.3 0.7 L0.7 9.3" /></svg>
      </button>
    </div>
  {/if}
</div>

<BriefingModal open={briefingOpen} onclose={() => (briefingOpen = false)} />

<style>
  .titlebar {
    position: relative;
    display: flex;
    align-items: center;
    height: 46px;
    flex-shrink: 0;
    padding-left: 14px;
    background: var(--bar-bg);
    border-bottom: 1px solid var(--border);
    user-select: none;
    -webkit-user-select: none;
  }
  .titlebar[data-platform="macos"] {
    padding-left: 0; /* mac-inset handles the leading space */
  }
  .mac-inset {
    width: 80px;
    height: 100%;
  } /* native traffic lights live here */
  .vdiv {
    width: 1px;
    height: 18px;
    background: var(--border);
    margin-right: 14px;
  }

  .brandchip {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex-shrink: 0;
    border-radius: 7px;
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .wordmark {
    margin-left: 9px;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--fg);
  }
  .chev {
    display: inline-flex;
    color: var(--fg4);
    margin: 0 3px;
  }
  .sdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    margin-right: 7px;
    flex-shrink: 0;
  }
  .crumb {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--fg2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }

  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 1px;
  }
  .titlebar[data-platform="macos"] .actions {
    padding-right: 10px; /* breathing room at the corner */
  }

  .caption {
    display: flex;
    height: 46px;
    margin-left: 4px;
  }
  .cap {
    width: 46px;
    height: 46px;
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--fg2);
    cursor: pointer;
  }
  .cap:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .cap.close:hover {
    background: #e81123;
    color: #fff;
  }
  .cap svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
  }
</style>
