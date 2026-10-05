<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Brief, Webhook } from "$lib/types";
  import { projects, isSyncableBrief } from "$lib/stores/projects.svelte";
  import { getCurrentWindow, type CursorIcon } from "@tauri-apps/api/window";
  import { settings, RAIL_WIDTH_DEFAULT, RAIL_WIDTH_MIN } from "$lib/stores/settings.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { openExternal, fireWebhook } from "$lib/tauri";
  import { relativeTime } from "$lib/time";
  import { statusColor, STATUS_ORDER, STATUS_LABEL } from "$lib/status";
  import { isStubBrief } from "$lib/bootstrap";
  import { collapsedSections, setSectionCollapsed, DESCRIPTION_SECTION } from "$lib/sections";
  import MarkdownView from "./MarkdownView.svelte";
  import IntegrationPanel from "./IntegrationPanel.svelte";
  import IntegrationsModal from "./IntegrationsModal.svelte";
  import BootstrapModal from "./BootstrapModal.svelte";
  import Icon from "./Icon.svelte";

  let { brief }: { brief: Brief } = $props();

  // This component is mounted under {#key brief.path}, so local state resets
  // automatically when the selection changes — no effects needed.
  let editing = $state(false);
  // svelte-ignore state_referenced_locally -- intentional: {#key brief.path} remounts this component, re-seeding draft from the new brief.
  let draft = $state(brief.raw);
  let saving = $state(false);
  let syncing = $state(false);
  let integrationsOpen = $state(false);
  // Auto-open Generate when this brief was just created (one-shot store signal).
  // {#key brief.path} remounts this component per selection, so reading +
  // clearing the flag here fires once for the new project and never again.
  // svelte-ignore state_referenced_locally -- intentional one-shot read on mount.
  const justCreated = projects.bootstrapPath === brief.path;
  if (justCreated) projects.bootstrapPath = null;
  let bootstrapOpen = $state(justCreated);

  // Header description folds to a single truncated line; remembered per brief
  // alongside the body's collapsed `##` sections.
  // svelte-ignore state_referenced_locally -- intentional: read once per mount ({#key brief.path}).
  let descCollapsed = $state(collapsedSections(brief.path).has(DESCRIPTION_SECTION));
  function toggleDescription() {
    descCollapsed = !descCollapsed;
    setSectionCollapsed(brief.path, DESCRIPTION_SECTION, descCollapsed);
  }

  // Status menu: the header's status pill opens a small anchored popover
  // listing the known statuses plus any custom ones already in the vault.
  // Picking one splices only the `status` frontmatter key (set_brief_status).
  let statusMenuOpen = $state(false);
  let statusSaving = $state(false);
  let statusMenuEl = $state<HTMLDivElement | null>(null);
  let statusOptions = $derived.by(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const s of [...STATUS_ORDER, ...projects.statuses]) {
      const k = s.toLowerCase();
      if (!seen.has(k)) {
        seen.add(k);
        out.push(s);
      }
    }
    return out;
  });
  function statusLabel(s: string): string {
    return STATUS_LABEL[s.toLowerCase()] ?? s;
  }
  function isCurrentStatus(s: string): boolean {
    return (brief.status ?? "").toLowerCase() === s.toLowerCase();
  }
  function toggleStatusMenu() {
    statusMenuOpen = !statusMenuOpen;
  }
  async function pickStatus(status: string) {
    statusMenuOpen = false;
    if (isCurrentStatus(status)) return;
    statusSaving = true;
    try {
      await projects.setStatus(brief.path, status);
      toasts.success(`Status → ${statusLabel(status)}`);
    } catch (e) {
      toasts.error(`Could not set status: ${e}`);
    } finally {
      statusSaving = false;
    }
  }
  // Close the status menu on outside click / Escape (listeners attached only
  // while it's open; document-level so clicks anywhere in the pane count).
  $effect(() => {
    if (!statusMenuOpen) return;
    const onPointer = (ev: PointerEvent) => {
      if (statusMenuEl && !statusMenuEl.contains(ev.target as Node)) statusMenuOpen = false;
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        statusMenuOpen = false;
      }
    };
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey, true);
    };
  });

  // A freshly-created brief still on its stub body → offer to generate one.
  let isStub = $derived(isStubBrief(brief));

  // Seed edit mode with the bootstrap draft; the existing Save is the commit
  // gate (the modal never writes to disk).
  function acceptDraft(raw: string) {
    draft = raw;
    editing = true;
    bootstrapOpen = false;
  }

  // Whether this brief has anything to sync (a GitHub link or explicit source).
  let syncable = $derived(isSyncableBrief(brief));
  // Whether an LLM synthesis provider is configured (enables Current State +
  // Open Questions on Refresh, even for briefs whose only signal is Captures).
  let canSynthesize = $derived(projects.llmProvider !== null);
  // Refresh is offered when there's deterministic data to sync OR a provider to
  // synthesize with.
  let canRefresh = $derived(syncable || canSynthesize);

  // Obsidian deep link (null unless the briefs dir is inside a vault).
  let obsidianUri = $derived(projects.obsidianUri(brief));
  // Briefs that wikilink to this one.
  let backlinks = $derived(projects.backlinksFor(brief.path));

  // WSL can't reach the Windows host without a URL handler; hint at the fix.
  let wslHint = $derived(
    projects.isWsl
      ? " WSL needs a URL handler — install wslu so wslview forwards links to Windows."
      : "",
  );

  // Pick a Material glyph for a launch link based on its label.
  function iconForLink(label: string): string {
    const l = label.toLowerCase();
    if (l.includes("github")) return "code";
    if (l.includes("docs") || l.includes("tauri")) return "menu_book";
    if (l.includes("obsidian")) return "hub";
    if (l.includes("claude")) return "auto_awesome";
    return "north_east";
  }

  async function openInObsidian() {
    if (!obsidianUri) return;
    try {
      await openExternal(obsidianUri);
    } catch (e) {
      toasts.error(
        `Could not open Obsidian (${e}). Make sure Obsidian is installed and this vault is open in it.${wslHint}`,
      );
    }
  }

  async function save() {
    saving = true;
    try {
      await projects.save(brief.path, draft);
      editing = false;
      toasts.success("Saved");
    } catch (e) {
      toasts.error(`Save failed: ${e}`);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    draft = brief.raw;
    editing = false;
  }

  // --- NeuroSkill labeled sessions ---------------------------------------
  // A brief with a NeuroSkill connection can mark a labeled work session, so
  // its EEG epochs attribute to this project. Start is fired at the moment of
  // intent (the first launch/open action, or the explicit button); end is fired
  // by the explicit button and, as safety nets, on navigating away (onDestroy)
  // — the backend's session cap is the final backstop.
  let hasNeuro = $derived(brief.connections.some((c) => c.provider === "neuroskill"));
  let sessionActive = $state(false);
  // Warn at most once per "daemon down" stretch so repeatedly launching into
  // work doesn't spam toasts; cleared the moment a label fires successfully.
  let neuroWarned = $state(false);

  async function startSession() {
    if (!hasNeuro || sessionActive) return;
    sessionActive = true;
    const ok = await projects.markSession(brief.path, "start");
    if (ok) {
      neuroWarned = false;
    } else {
      // The label never reached NeuroSkill — don't leave a false "recording"
      // state, and tell the user so tracking isn't silently lost.
      sessionActive = false;
      if (!neuroWarned) {
        neuroWarned = true;
        toasts.error(
          "Couldn't reach NeuroSkill — session not started, so EEG won't attribute to this project. Is the NeuroSkill app running?",
        );
      }
    }
  }

  function endSession() {
    if (!sessionActive) return;
    sessionActive = false;
    projects.markSession(brief.path, "end").then((ok) => {
      if (!ok)
        toasts.error(
          "Couldn't reach NeuroSkill — the session end wasn't recorded (it auto-closes after 4h).",
        );
    });
  }

  // Safety net: end an open session when this brief is closed/switched away
  // (the component remounts per selection under {#key brief.path}) or on app
  // close. Silent — navigating away shouldn't pop a toast; the 4h cap backstops it.
  onDestroy(() => {
    if (sessionActive) void projects.markSession(brief.path, "end");
  });

  async function openLink(url: string) {
    // Opening a link IS launching into work — start the labeled session once.
    startSession();
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(`Could not open link (${e}).${wslHint}`);
    }
  }

  async function fire(hook: Webhook) {
    startSession();
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

  async function refresh() {
    if (syncing) return;
    syncing = true;
    try {
      // Deterministic Activity sync first, then (when configured) LLM synthesis
      // of Current State + Open Questions. Each step is independent so a sync
      // failure still reports clearly.
      if (syncable) await projects.sync(brief.path);
      if (canSynthesize) await projects.synthesize(brief.path);
      toasts.success(canSynthesize ? "Refreshed" : "Synced");
    } catch (e) {
      toasts.error(`Refresh failed: ${e}`);
    } finally {
      syncing = false;
    }
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "s") {
      e.preventDefault();
      save();
    }
  }

  function manage() {
    integrationsOpen = true;
  }

  // Two-column layout: drag the rail's left border to resize it, between
  // RAIL_WIDTH_MIN and half of the row. Width persists via the settings store.
  let railRow = $state<HTMLDivElement | null>(null);
  let railDragging = $state(false);

  // The rail's hard cap is half the row (the CSS `max-width: 50%`), tracked
  // with a ResizeObserver so it follows the window. At narrow widths (the
  // window allows 720px) that cap drops below RAIL_WIDTH_MIN, so the
  // *achievable* bounds and the visible width are derived from it rather
  // than from the saved preference: the separator reports what the user
  // actually sees, and keyboard moves start from there.
  let railCap = $state(RAIL_WIDTH_DEFAULT);
  $effect(() => {
    if (!railRow) return;
    const ro = new ResizeObserver(([entry]) => {
      railCap = Math.max(1, Math.floor(entry.contentRect.width * 0.5));
    });
    ro.observe(railRow);
    return () => ro.disconnect();
  });
  const railMin = $derived(Math.min(RAIL_WIDTH_MIN, railCap));
  const railNow = $derived(Math.min(settings.railWidth, railCap));

  function clampRail(px: number): number {
    return Math.min(railCap, Math.max(railMin, px));
  }

  // Keyboard resize for the separator (WAI-ARIA "window splitter"): arrows
  // nudge by 16px (64px with Shift), Home/End jump to the bounds, Enter
  // resets like a double-click. The rail sits on the right, so ArrowLeft
  // moves the splitter left and widens it. A move that cannot change the
  // visible width is dropped, so a cramped window never rewrites the saved
  // preference.
  const RAIL_KEY_STEP = 16;
  function onRailKeydown(e: KeyboardEvent) {
    const step = e.shiftKey ? RAIL_KEY_STEP * 4 : RAIL_KEY_STEP;
    let next: number;
    switch (e.key) {
      case "ArrowLeft":
      case "ArrowUp":
        next = railNow + step;
        break;
      case "ArrowRight":
      case "ArrowDown":
        next = railNow - step;
        break;
      case "Home":
        next = railMin;
        break;
      case "End":
        next = railCap;
        break;
      case "Enter":
        next = RAIL_WIDTH_DEFAULT;
        break;
      default:
        return;
    }
    e.preventDefault();
    next = clampRail(next);
    if (next === railNow) return;
    settings.setRailWidth(next);
  }

  // WebKitGTK (Linux/WSL) often ignores the CSS `cursor` (see ResizeHandles),
  // so drive the native cursor too. No-op outside Tauri (plain `vite dev`).
  function setCursor(icon: CursorIcon) {
    try {
      getCurrentWindow().setCursorIcon(icon).catch(() => {});
    } catch {
      // Window APIs unavailable.
    }
  }

  function startRailDrag(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    railDragging = true;
  }

  function moveRailDrag(e: PointerEvent) {
    if (!railDragging || !railRow) return;
    const r = railRow.getBoundingClientRect();
    const next = clampRail(r.right - e.clientX);
    // Same guard as the keyboard path: when the visible width cannot change
    // (cramped window), don't let the store's floor overwrite the preference.
    if (next === railNow) return;
    settings.setRailWidth(next, false);
  }

  function endRailDrag(e: PointerEvent) {
    if (!railDragging) return;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    railDragging = false;
    settings.setRailWidth(settings.railWidth);
    setCursor("default");
  }
</script>

<!-- A hairline section label with an optional right-aligned "Manage" link. -->
{#snippet sectionLabel(text: string, withManage: boolean)}
  <div class="mb-3 flex items-center">
    <span class="label">{text}</span>
    <span class="ml-3 h-px flex-1" style="background: var(--border);"></span>
    {#if withManage}
      <button class="btn-quiet" style="font-size: 11.5px;" onclick={manage}>Manage</button>
    {/if}
  </div>
{/snippet}

<!-- Stub-brief CTA: a compact accent-tinted card to draft the initial brief. -->
{#snippet generateCTA()}
  <div
    class="mb-[22px] flex items-center gap-[13px] rounded-[13px] border"
    style="border-color: var(--accent-line); background: var(--accent-tint); padding: 12px 14px;"
  >
    <span class="grid h-[34px] w-[34px] shrink-0 place-items-center rounded-[10px] text-white" style="background: var(--accent);">
      <Icon name="auto_awesome" size={18} />
    </span>
    <div class="min-w-0 flex-1">
      <div class="text-[13px] font-semibold text-[var(--fg)]">Generate the initial brief</div>
      <div class="mt-px text-[11.5px] leading-[1.45] text-[var(--fg2)]">
        Draft from a folder, a repo, a few answers, or a pasted prompt — review before saving.
      </div>
    </div>
    <button class="btn btn-primary btn-sm shrink-0" onclick={() => (bootstrapOpen = true)}>
      <Icon name="bolt" size={14} fill={1} /> Generate
    </button>
  </div>
{/snippet}

<!-- The brief body plus its "Linked from" backlinks. -->
{#snippet bodyAndBacklinks()}
  <MarkdownView source={brief.body} collapseKey={brief.path} />

  {#if backlinks.length}
    <section class="mt-[30px] border-t pt-[18px]" style="border-color: var(--border);">
      <h3 class="label mb-[10px]">Linked from</h3>
      <div class="flex flex-wrap gap-[7px]">
        {#each backlinks as link (link.path)}
          <button
            class="chip cursor-pointer transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
            onclick={() => projects.select(link.path)}
          >
            <Icon name="subdirectory_arrow_right" size={13} /> {link.name}
          </button>
        {/each}
      </div>
    </section>
  {/if}
{/snippet}

<div class="flex h-screen flex-col bg-[var(--bg)] text-[var(--fg)]">
  <!-- Header -->
  <header class="shrink-0 border-b" style="border-color: var(--border); padding: var(--pane-py) var(--pane-px) 16px;">
    <!-- Title row -->
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <div class="flex items-center gap-[9px]">
          <h2 class="truncate font-bold tracking-[-0.02em] text-[var(--fg)]" style="font-size: var(--title-size);">
            {brief.name}
          </h2>
          <!-- Status pill doubles as the status toggle. -->
          <div class="status-menu relative shrink-0" bind:this={statusMenuEl}>
            <button
              type="button"
              class="pill status-trigger"
              class:open={statusMenuOpen}
              style="--sc: {statusColor(brief.status)};"
              title="Change status"
              aria-haspopup="listbox"
              aria-expanded={statusMenuOpen}
              disabled={statusSaving || editing}
              onclick={toggleStatusMenu}
            >
              <span class="dot"></span>
              {brief.status ? statusLabel(brief.status) : "No status"}
              <Icon name="expand_more" size={12} />
            </button>
            {#if statusMenuOpen}
              <div class="status-popover" role="listbox" aria-label="Project status">
                {#each statusOptions as option (option.toLowerCase())}
                  {@const current = isCurrentStatus(option)}
                  <button
                    type="button"
                    class="status-option"
                    class:current
                    role="option"
                    aria-selected={current}
                    onclick={() => pickStatus(option)}
                  >
                    <span class="sdot h-[7px] w-[7px]" style="--sc: {statusColor(option)};"></span>
                    <span class="flex-1 text-left">{statusLabel(option)}</span>
                    {#if current}
                      <Icon name="check" size={13} />
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
        {#if brief.description}
          <button
            type="button"
            class="desc-toggle mt-[7px] flex max-w-[60ch] items-start gap-[7px] text-left"
            class:open={!descCollapsed}
            aria-expanded={!descCollapsed}
            title={descCollapsed ? "Expand description" : "Collapse description"}
            onclick={toggleDescription}
          >
            <p class="min-w-0 text-[13.5px] leading-[1.5] text-[var(--fg2)]" class:truncate={descCollapsed}>
              {brief.description}
            </p>
          </button>
        {/if}
      </div>

      <div class="flex shrink-0 gap-[6px]">
        {#if editing}
          <button class="btn" onclick={cancel} disabled={saving}>Cancel</button>
          <button class="btn btn-primary" onclick={save} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        {:else}
          {#if canRefresh}
            <button
              class="btn"
              title={canSynthesize
                ? "Refresh activity and re-synthesize Current State + Open Questions"
                : "Refresh synced data from linked integrations"}
              onclick={refresh}
              disabled={syncing}
            >
              <Icon name="sync" size={14} class={syncing ? "spin" : ""} />
              {syncing ? "Refreshing…" : "Refresh"}
            </button>
          {/if}
          <button class="btn" title="Manage this brief's integrations (Linear, Jira, …)" onclick={manage}>
            <Icon name="hub" size={14} /> Integrations
            {#if brief.integrations.length}
              <span class="rounded-full bg-[var(--chip-bg)] px-[6px] text-[10px] font-semibold text-[var(--fg2)]">
                {brief.integrations.length}
              </span>
            {/if}
          </button>
          {#if hasNeuro}
            {#if sessionActive}
              <button
                class="btn"
                title="End the labeled NeuroSkill work session for this project"
                onclick={endSession}
              >
                <Icon name="stop_circle" size={14} fill={1} /> End session
              </button>
            {:else}
              <button
                class="btn"
                title="Start a labeled NeuroSkill work session so EEG attributes to this project"
                onclick={startSession}
              >
                <Icon name="neurology" size={14} /> Start session
              </button>
            {/if}
          {/if}
          {#if obsidianUri}
            <button class="btn" title="Open this brief in Obsidian" onclick={openInObsidian}>
              <Icon name="hub" size={14} /> Obsidian
            </button>
          {/if}
          <button
            class="btn"
            onclick={() => {
              draft = brief.raw;
              editing = true;
            }}
          >
            <Icon name="edit" size={14} /> Edit
          </button>
        {/if}
      </div>
    </div>

    <!-- Tags + opened/synced meta -->
    <div class="mt-[14px] flex flex-wrap items-center gap-3">
      {#each brief.tags as tag (tag)}
        <span class="tag">{tag}</span>
      {/each}
      <span class="inline-flex items-center gap-[5px] text-[11.5px] text-[var(--fg3)]">
        <Icon name="schedule" size={13} /> Opened {relativeTime(brief.lastOpened)}
      </span>
      {#if brief.lastSynced}
        <span class="inline-flex items-center gap-[5px] text-[11.5px] text-[var(--fg3)]">
          <Icon name="sync" size={13} /> Synced {relativeTime(brief.lastSynced)}
        </span>
      {/if}
    </div>

    <!-- Launch row: links + webhooks -->
    {#if brief.links.length || brief.webhooks.length}
      <div class="mt-[14px] flex flex-wrap gap-[7px]">
        {#each brief.links as link (link.url + link.label)}
          <button
            class="inline-flex h-[29px] items-center gap-[7px] rounded-[8px] border px-[11px] text-[12px] font-medium transition-colors hover:bg-[var(--hover)]"
            style="border-color: var(--border); background: var(--bg); color: var(--fg-body);"
            onclick={() => openLink(link.url)}
          >
            <Icon name={iconForLink(link.label || link.url)} size={14} class="text-[var(--fg2)]" />
            {link.label || link.url}
            <Icon name="north_east" size={11} class="text-[var(--fg4)]" />
          </button>
        {/each}
        {#each brief.webhooks as hook (hook.url + hook.label)}
          <button
            class="inline-flex h-[29px] items-center gap-[7px] rounded-[8px] border px-[11px] text-[12px] font-medium transition-[filter] hover:brightness-[1.04]"
            style="border-color: var(--hook-bd); background: var(--hook-bg); color: var(--hook-fg);"
            title={`${hook.method} ${hook.url}`}
            onclick={() => fire(hook)}
          >
            <Icon name="bolt" size={14} fill={1} />
            {hook.label || hook.url}
            <span class="text-[9px] font-bold uppercase opacity-70">{hook.method}</span>
          </button>
        {/each}
      </div>
    {/if}
  </header>

  <!-- Body: editor, or one of the three live-state layouts -->
  {#if editing}
    <div class="scroll-thin min-h-0 flex-1 overflow-y-auto" style="padding: var(--pane-py) var(--pane-px);">
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="h-full min-h-[56vh] w-full resize-none rounded-[10px] border p-4 font-mono text-[12.5px] leading-[1.65] text-[var(--fg-body)] outline-none focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-tint)]"
        style="background: var(--input-bg); border-color: var(--border);"
        bind:value={draft}
        onkeydown={onEditorKeydown}
        spellcheck="false"
        autofocus
      ></textarea>
      <p class="mt-2 text-[11px] text-[var(--fg3)]">
        Editing the raw file (frontmatter + markdown). ⌘/Ctrl+S to save.
      </p>
    </div>
  {:else if settings.briefLayout === "two-col"}
    <!-- Two-column: brief body left, live-state rail right. Each column is
         its own scroll container, so a long brief and a long feed scroll
         independently. -->
    <div
      bind:this={railRow}
      class="flex min-h-0 flex-1 items-stretch"
      class:select-none={railDragging}
    >
      <div class="scroll-thin min-w-0 flex-1 overflow-y-auto" style="padding: var(--pane-py) var(--pane-px);">
        {#if isStub}{@render generateCTA()}{/if}
        {@render bodyAndBacklinks()}
      </div>
      <!-- max-width keeps the 50% cap true when the window shrinks, without
           rewriting the saved width. The grip sits on the non-scrolling
           aside so it spans the full height; the content scrolls inside. -->
      <aside
        class="relative flex min-h-0 shrink-0 flex-col border-l"
        style="width: {settings.railWidth}px; max-width: 50%; border-color: var(--border); background: var(--rail-bg);"
      >
        <!-- A focusable separator: pointer drag, or arrow/Home/End/Enter keys. -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
        <div
          class="rail-grip"
          class:dragging={railDragging}
          role="separator"
          tabindex="0"
          aria-orientation="vertical"
          aria-label="Resize live state panel"
          aria-valuemin={railMin}
          aria-valuenow={railNow}
          aria-valuemax={railCap}
          title="Drag to resize · double-click to reset · arrow keys to adjust"
          onkeydown={onRailKeydown}
          onpointerdown={startRailDrag}
          onpointermove={moveRailDrag}
          onpointerup={endRailDrag}
          onpointercancel={endRailDrag}
          ondblclick={() => settings.setRailWidth(RAIL_WIDTH_DEFAULT)}
          onmouseenter={() => setCursor("ewResize")}
          onmouseleave={() => !railDragging && setCursor("default")}
        ></div>
        <div class="scroll-thin min-h-0 flex-1 overflow-y-auto" style="padding: var(--pane-py) 22px;">
          {@render sectionLabel("Live state", true)}
          <IntegrationPanel {brief} narrow onManage={manage} />
        </div>
      </aside>
    </div>
  {:else if settings.briefLayout === "body"}
    <!-- Body first, then the live-state block below a hairline divider. -->
    <div class="scroll-thin min-h-0 flex-1 overflow-y-auto" style="padding: var(--pane-py) var(--pane-px);">
      {#if isStub}{@render generateCTA()}{/if}
      {@render bodyAndBacklinks()}
      <div style="margin-top: 34px; padding-top: 26px; border-top: 1px solid var(--border);">
        {@render sectionLabel("Live state", true)}
        <IntegrationPanel {brief} onManage={manage} />
      </div>
    </div>
  {:else}
    <!-- Quiet top: a slim feed strip (or connect row), then the body. -->
    <div class="scroll-thin min-h-0 flex-1 overflow-y-auto" style="padding: var(--pane-py) var(--pane-px);">
      <div class="mb-[26px]">
        <IntegrationPanel {brief} strip onManage={manage} />
      </div>
      {#if isStub}{@render generateCTA()}{/if}
      {@render bodyAndBacklinks()}
    </div>
  {/if}

  <footer
    class="flex gap-[14px] border-t font-mono text-[10.5px] text-[var(--fg3)]"
    style="border-color: var(--border); padding: 7px var(--pane-px);"
  >
    <span class="truncate">{brief.path}</span>
    <span class="ml-auto shrink-0">⌘K capture · ⌘S save · ⌘F search</span>
  </footer>
</div>

{#if integrationsOpen}
  <IntegrationsModal {brief} onclose={() => (integrationsOpen = false)} />
{/if}

{#if bootstrapOpen}
  <BootstrapModal {brief} onclose={() => (bootstrapOpen = false)} onDraft={acceptDraft} />
{/if}

<style>
  /* Grab strip straddling the rail's left border; the visible hairline is the
     ::after so the hit area stays generous. */
  .rail-grip {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -4px;
    width: 8px;
    z-index: 5;
    cursor: col-resize;
    touch-action: none;
  }
  .rail-grip::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 2px;
    background: transparent;
    transition: background 120ms ease;
  }
  .rail-grip:hover::after {
    background: var(--accent-line);
  }
  .rail-grip.dragging::after,
  .rail-grip:focus-visible::after {
    background: var(--accent);
  }
  .rail-grip:focus-visible {
    outline: none;
  }

  /* Status pill as a toggle: same look as the read-only pill, plus a hover
     ring and a tiny chevron so it reads as interactive. */
  .status-trigger {
    cursor: pointer;
    border: 1px solid transparent;
    transition: border-color 120ms ease, filter 120ms ease;
  }
  .status-trigger:hover,
  .status-trigger.open {
    border-color: color-mix(in srgb, var(--sc) 45%, transparent);
  }
  .status-trigger:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .status-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 20;
    min-width: 150px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
  }
  .status-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 6px;
    font-size: 12.5px;
    color: var(--fg);
    cursor: pointer;
    text-transform: capitalize;
  }
  .status-option:hover {
    background: var(--hover);
  }
  .status-option.current {
    color: var(--fg2);
  }
</style>
