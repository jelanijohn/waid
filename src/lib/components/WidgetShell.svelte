<script lang="ts">
  // Widget mode root: rest line / roster (+ leaf). Owns the window-focus
  // listeners, the content ResizeObserver that sizes the window, and position
  // memory. Layout never depends on the window's size (fixed px widths, auto
  // heights), so resizing the window can't feed back into the measurement.
  import { onMount, tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { Brief } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { session } from "$lib/stores/session.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { widget } from "$lib/stores/widget.svelte";
  import { integrations } from "$lib/stores/integrations.svelte";
  import { appendCapture } from "$lib/tauri";
  import { syncAllProjects } from "$lib/actions";
  import { statusColor } from "$lib/status";
  import { statusTally } from "$lib/widget";
  import { platform, paintsOwnFrame } from "$lib/platform";
  import {
    GUTTER,
    LEAF_W,
    LEAF_GAP,
    setWidgetSize,
    reapplyWidgetSize,
    windowPosition,
  } from "$lib/widgetWindow";
  import WidgetRow from "./WidgetRow.svelte";
  import WidgetLeaf from "./WidgetLeaf.svelte";
  import BrandMark from "./BrandMark.svelte";
  import Icon from "./Icon.svelte";

  const isMac = platform === "macos";
  const panelClass = `overflow-hidden bg-[var(--bg)] text-[var(--fg)] ${
    paintsOwnFrame ? "rounded-[12px] border border-[var(--border)] shadow-[var(--shadow-win)]" : ""
  }`;

  let measured = $state<HTMLDivElement | null>(null);
  let captureEl = $state<HTMLInputElement | null>(null);
  let syncing = $state(false);
  let capturing = $state(false);

  let lead = $derived(widget.lead);
  let roster = $derived(widget.roster);
  let tally = $derived(statusTally(projects.briefs));
  /** Capture goes to the open leaf's brief, else the lead. */
  let captureTarget = $derived(widget.openBrief ?? lead);

  // --- Sizing --------------------------------------------------------------
  let frame = 0;
  function measure(): number {
    return measured ? Math.ceil(measured.getBoundingClientRect().height) : 0;
  }
  function scheduleResize() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (widget.shifting || widget.mode !== "widget") return;
      void setWidgetSize(widget.contentW, measure());
    });
  }

  $effect(() => {
    if (!measured) return;
    widget.measure = measure;
    const ro = new ResizeObserver(scheduleResize);
    ro.observe(measured);
    return () => {
      ro.disconnect();
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      widget.measure = null;
    };
  });

  // A resize skipped while a re-layout was in flight is caught up here.
  $effect(() => {
    if (!widget.shifting) scheduleResize();
  });

  // --- Focus, scale, position memory ---------------------------------------
  let moveTimer: ReturnType<typeof setTimeout> | null = null;
  function rememberPosition() {
    if (moveTimer) clearTimeout(moveTimer);
    moveTimer = setTimeout(async () => {
      moveTimer = null;
      if (widget.shifting || widget.mode !== "widget") return;
      const pos = await windowPosition();
      if (!pos || widget.shifting) return;
      // Store the roster's anchor, not the window origin: a left side leaf
      // sits in front of the roster.
      const offset = widget.sideOpen && widget.leafSide === "left" ? LEAF_W + LEAF_GAP : 0;
      settings.setWidgetPos({ x: pos.x + offset, y: pos.y });
    }, 400);
  }

  onMount(() => {
    const onFocus = () => widget.setFocused(true);
    const onBlur = () => widget.setFocused(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && widget.openPath) {
        e.preventDefault();
        void widget.closeLeaf();
      }
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    window.addEventListener("keydown", onKey);
    widget.setFocused(document.hasFocus());

    const unlisten: (() => void)[] = [];
    let disposed = false;
    (async () => {
      try {
        const win = getCurrentWindow();
        const subs = await Promise.all([
          win.onFocusChanged(({ payload }) => widget.setFocused(payload)),
          win.onScaleChanged(() => void reapplyWidgetSize()),
          win.onMoved(() => {
            if (!widget.shifting) rememberPosition();
          }),
        ]);
        if (disposed) subs.forEach((u) => u());
        else unlisten.push(...subs);
      } catch {
        // Window APIs unavailable (plain `vite dev`): DOM focus events still work.
      }
    })();

    return () => {
      disposed = true;
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("keydown", onKey);
      unlisten.forEach((u) => u());
      if (moveTimer) clearTimeout(moveTimer);
    };
  });

  // The click that wakes the widget from rest must not also land on whatever
  // the roster just rendered under the pointer (a row, Sync, Expand). The
  // roster appears on pointerdown (or the OS focus event just before it), so
  // a press that began at rest — or within a beat of waking — is swallowed.
  const WAKE_GUARD_MS = 350;
  let pressFromRest = false;
  function onPointerDownCapture() {
    pressFromRest = !widget.focused || performance.now() - widget.focusedAt < WAKE_GUARD_MS;
    widget.setFocused(true);
  }
  function onClickCapture(e: MouseEvent) {
    if (!pressFromRest) return;
    pressFromRest = false;
    e.preventDefault();
    e.stopPropagation();
  }

  // --- Capture ---------------------------------------------------------------
  let seenTick = widget.captureFocusTick;
  $effect(() => {
    const t = widget.captureFocusTick;
    if (t === seenTick) return;
    seenTick = t;
    tick().then(() => captureEl?.focus());
  });

  async function submitCapture() {
    const text = widget.captureDraft.trim();
    const target = captureTarget;
    if (!text || !target || capturing) return;
    capturing = true;
    try {
      const updated = await appendCapture(target.path, text);
      projects.upsert(updated);
      toasts.push("Captured 🎉", "success");
      widget.captureDraft = "";
    } catch (e) {
      toasts.error(`Capture failed: ${e}`);
    } finally {
      capturing = false;
    }
  }

  async function syncAll() {
    if (syncing) return;
    syncing = true;
    try {
      await syncAllProjects();
    } finally {
      syncing = false;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 overflow-hidden"
  style="padding: {GUTTER}px;"
  onpointerdowncapture={onPointerDownCapture}
  onclickcapture={onClickCapture}
  onkeydown={() => widget.setFocused(true)}
>
  <div
    bind:this={measured}
    class="flex {paintsOwnFrame ? 'items-start' : 'items-stretch'}"
    style="width: {widget.contentW}px; gap: {LEAF_GAP}px; visibility: {widget.shifting ? 'hidden' : 'visible'};"
  >
    {#if widget.sideOpen && widget.openBrief && widget.leafSide === "left"}
      {@render sidePanel(widget.openBrief)}
    {/if}
    <section class="roster flex shrink-0 flex-col {panelClass}" aria-label="WAID widget">
      <header class="head" data-tauri-drag-region>
        {#if isMac}<span class="mac-inset" data-tauri-drag-region></span>{/if}
        <span class="brand" data-tauri-drag-region>
          <BrandMark size={18} />
          <span class="word" data-tauri-drag-region>WAID</span>
        </span>
        <span class="flex-1" data-tauri-drag-region></span>
        {#if widget.focused}
          <button class="hbtn" title="Sync all projects" aria-label="Sync all projects" onclick={syncAll} disabled={syncing}>
            <Icon name="sync" size={16} class={syncing ? "spin" : ""} />
          </button>
          <button class="hbtn" title="Expand to dashboard" aria-label="Expand to dashboard" onclick={() => widget.exit()}>
            <Icon name="open_in_full" size={16} />
          </button>
        {:else}
          <span class="tally" data-tauri-drag-region>
            {#if integrations.totalFresh > 0}
              <span class="tcount fresh" title="{integrations.totalFresh} new across your projects">
                <span class="tdot" style="background: var(--accent);"></span>{integrations.totalFresh} new
              </span>
            {/if}
            {#each tally as t (t.status)}
              <span class="tcount" title={t.status}>
                <span class="tdot" style="background: {statusColor(t.status)};"></span>{t.count}
              </span>
            {/each}
          </span>
        {/if}
      </header>

      {#if !widget.focused}
        <!-- Rest: one button that brings the roster back. -->
        <button type="button" class="rest" aria-label="Show all projects" onclick={() => widget.setFocused(true)}>
          {#if lead}
            <WidgetRow brief={lead} lead still inSession={session.isActive(lead.path)} fresh={integrations.freshCount(lead.path)} />
          {:else}
            <span class="empty">No briefs yet</span>
          {/if}
        </button>
      {:else}
        <div class="list {roster.length > 8 ? 'capped scroll-thin' : ''}">
          {#each roster as b, i (b.path)}
            <WidgetRow
              brief={b}
              lead={i === 0}
              inSession={session.isActive(b.path)}
              fresh={integrations.freshCount(b.path)}
              open={widget.openPath === b.path}
              side={widget.sideOpen ? widget.leafSide : null}
              onToggle={() => widget.toggleLeaf(b.path)}
            />
            {#if widget.openPath === b.path && !widget.openAsSide}
              {#key b.path}
                <WidgetLeaf brief={b} variant="accordion" />
              {/key}
            {/if}
          {:else}
            <span class="empty">No briefs yet</span>
          {/each}
        </div>

        <footer class="capture" class:mt-auto={!paintsOwnFrame}>
          <Icon name="edit" size={16} class="text-[var(--fg2)]" />
          <label for="widget-capture" class="sr-only">Capture a note</label>
          <input
            id="widget-capture"
            bind:this={captureEl}
            bind:value={widget.captureDraft}
            placeholder={captureTarget ? `Capture a note to ${captureTarget.name}` : "No brief to capture into"}
            disabled={!captureTarget || capturing}
            autocomplete="off"
            spellcheck="false"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void submitCapture();
              }
            }}
          />
          <kbd>{isMac ? "⌘K" : "Ctrl K"}</kbd>
        </footer>
      {/if}
    </section>
    {#if widget.sideOpen && widget.openBrief && widget.leafSide === "right"}
      {@render sidePanel(widget.openBrief)}
    {/if}
  </div>
</div>

{#snippet sidePanel(b: Brief)}
  <!-- On Windows there's no gap between panels (the OS frames the whole
       window), so the leaf is a column set off by a hairline instead. -->
  <section
    class="sidepanel shrink-0 {panelClass} scroll-thin"
    class:divided={!paintsOwnFrame}
    class:on-left={widget.leafSide === "left"}
    aria-label="{b.name} details"
  >
    {#key b.path}
      <WidgetLeaf brief={b} variant="side" />
    {/key}
  </section>
{/snippet}

<style>
  .roster {
    width: 300px;
  }
  .sidepanel {
    width: 340px;
    padding: 14px;
    max-height: 560px;
    overflow-y: auto;
  }
  .sidepanel.divided {
    border-left: 1px solid var(--border);
  }
  .sidepanel.divided.on-left {
    border-left: none;
    border-right: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 12px 12px 8px;
    user-select: none;
    -webkit-user-select: none;
  }
  .mac-inset {
    width: 80px;
    flex-shrink: 0;
    align-self: stretch;
  } /* native traffic lights live here */
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .word {
    font-size: 15px;
    font-weight: 500;
    color: var(--fg-body);
  }
  .hbtn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--fg2);
    cursor: pointer;
  }
  .hbtn:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .hbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .tally {
    display: inline-flex;
    align-items: center;
    gap: 12px;
  }
  .tcount {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--fg2);
    font-variant-numeric: tabular-nums;
  }
  .tcount.fresh {
    color: var(--accent);
    font-weight: 600;
  }
  .tdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .rest {
    display: block;
    width: 100%;
    padding: 0 0 4px;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .empty {
    display: block;
    padding: 6px 12px 10px;
    font-size: 13px;
    color: var(--fg2);
  }
  .list {
    padding-bottom: 4px;
  }
  .list.capped {
    max-height: 240px;
    overflow-y: auto;
  }
  .capture {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
  }
  .capture input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    font-size: 12px;
    color: var(--fg-body);
  }
  .capture input::placeholder {
    color: var(--fg2);
  }
  kbd {
    flex-shrink: 0;
    padding: 2px 5px;
    border: 1px solid var(--border);
    border-radius: 5px;
    font-family: inherit;
    font-size: 10px;
    font-weight: 600;
    color: var(--fg2);
  }
</style>
