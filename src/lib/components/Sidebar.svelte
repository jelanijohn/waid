<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { relativeTime } from "$lib/time";
  import { createBrief, pickDirectory, setBriefsDir } from "$lib/tauri";
  import { settings } from "$lib/stores/settings.svelte";
  import { widget } from "$lib/stores/widget.svelte";
  import { STATUS_ORDER, STATUS_LABEL, statusColor } from "$lib/status";
  import StatusPill from "./StatusPill.svelte";
  import Icon from "./Icon.svelte";
  import BrandMark from "./BrandMark.svelte";

  let creating = $state(false);
  let newName = $state("");
  let searchEl = $state<HTMLInputElement>();

  onMount(() => {
    // ⌘/Ctrl+F focuses the search box; ⌘/Ctrl+N starts a new project.
    const onKey = (e: KeyboardEvent) => {
      // The sidebar is hidden (still mounted) in widget mode.
      if (widget.mode === "widget") return;
      const k = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && k === "f") {
        e.preventDefault();
        searchEl?.focus();
        searchEl?.select();
      } else if ((e.metaKey || e.ctrlKey) && k === "n") {
        e.preventDefault();
        startNew();
      } else if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        // Keyboard counterpart to drag-reordering: nudge the selected project.
        const t = e.target as HTMLElement | null;
        if (
          t &&
          (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable)
        )
          return;
        if (!projects.selectedPath) return;
        e.preventDefault();
        nudge(projects.selectedPath, e.key === "ArrowUp" ? -1 : 1);
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
      const s = groupKey(b);
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

  /** The status bucket a brief lands in for the grouped (compact/rocks) views. */
  function groupKey(b: Brief): string {
    return (b.status ?? "").toLowerCase() || "other";
  }

  // --- Drag-and-drop reordering -------------------------------------------
  // Native HTML5 DnD (dragDropEnabled is off in tauri.conf.json so the webview
  // doesn't swallow it). Rows are draggable in every view; in the grouped views
  // a row can only be dropped within its own status group. The order itself is
  // one global list persisted by the backend (see projects.reorder).

  let dragPath = $state<string | null>(null);
  let dropTarget = $state<{ path: string; after: boolean } | null>(null);

  /** The rows a brief is currently displayed alongside (its drop candidates). */
  function siblingsOf(path: string): Brief[] {
    if (settings.sidebarStyle === "rows") return projects.filtered;
    const brief = projects.briefs.find((b) => b.path === path);
    if (!brief) return [];
    const key = groupKey(brief);
    return groups.find((g) => g.status === key)?.items ?? [];
  }

  /** Whether landing `from` before/after `to` would actually change the order
   *  as displayed (dropping a row right back into its own slot is a no-op). */
  function wouldMove(from: string, to: string, after: boolean): boolean {
    const sibs = siblingsOf(from);
    const fi = sibs.findIndex((b) => b.path === from);
    const ti = sibs.findIndex((b) => b.path === to);
    if (fi === -1 || ti === -1) return false;
    const slot = after ? ti + 1 : ti;
    return slot !== fi && slot !== fi + 1;
  }

  function onDragStart(e: DragEvent, brief: Brief) {
    dragPath = brief.path;
    dropTarget = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", brief.path);
    }
  }

  function onDragOver(e: DragEvent, brief: Brief) {
    if (!dragPath || dragPath === brief.path) return;
    const el = e.currentTarget as HTMLElement;
    const r = el.getBoundingClientRect();
    const after = e.clientY > r.top + r.height / 2;
    if (!wouldMove(dragPath, brief.path, after)) {
      dropTarget = null;
      return; // not preventing default → browser shows "no drop"
    }
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    if (dropTarget?.path !== brief.path || dropTarget.after !== after) {
      dropTarget = { path: brief.path, after };
    }
  }

  async function onDrop(e: DragEvent, brief: Brief) {
    e.preventDefault();
    const from = dragPath;
    const target = dropTarget;
    dragPath = null;
    dropTarget = null;
    if (!from || !target || target.path !== brief.path) return;
    try {
      await projects.reorder(from, target.path, target.after);
    } catch (err) {
      toasts.error(`Could not save project order: ${err}`);
    }
  }

  function onDragEnd() {
    dragPath = null;
    dropTarget = null;
  }

  /** Clear the indicator when the drag leaves the list entirely. */
  function onListDragLeave(e: DragEvent) {
    const nav = e.currentTarget as HTMLElement;
    const to = e.relatedTarget as Node | null;
    if (!to || !nav.contains(to)) dropTarget = null;
  }

  /** Move a brief one slot up/down among its displayed siblings. */
  async function nudge(path: string, delta: -1 | 1) {
    const sibs = siblingsOf(path);
    const i = sibs.findIndex((b) => b.path === path);
    const target = sibs[i + delta];
    if (i === -1 || !target) return;
    try {
      await projects.reorder(path, target.path, delta === 1);
    } catch (err) {
      toasts.error(`Could not save project order: ${err}`);
    }
  }

  /** Classes for a draggable row: dimmed while dragged, drop line when targeted. */
  function dragClass(path: string): string {
    if (dragPath === path) return "is-dragging";
    if (dropTarget?.path === path) return dropTarget.after ? "drop-after" : "drop-before";
    return "";
  }

  function groupLabel(status: string): string {
    return STATUS_LABEL[status] ?? status.charAt(0).toUpperCase() + status.slice(1);
  }

  function setStatusFilter(value: string) {
    projects.statusFilter = value === "" ? null : value;
  }

  // The dropdown's options: every status present, plus the active filter if
  // it no longer matches any brief (e.g. the only "blocked" brief was just
  // re-labelled from the header picker). Keeping it listed keeps the
  // "All statuses" control reachable so the user can clear the empty view.
  let filterOptions = $derived.by(() => {
    const f = projects.statusFilter;
    const present = projects.statuses;
    if (!f || present.some((s) => s.toLowerCase() === f.toLowerCase())) return present;
    return [...present, f];
  });

  function startNew() {
    newName = "";
    creating = true;
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
</script>

<aside
  class="flex h-full w-[270px] shrink-0 flex-col border-r bg-[var(--side-bg)]"
  style="border-color: var(--border);"
>
  <!-- Search (the sidebar's first element now the brand lives in the toolbar) -->
  {#if projects.briefs.length > 0}
    <div class="px-3 pb-[6px] pt-[14px]">
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

    <!-- Status filter dropdown: shown when there's something to filter by,
         and always while a filter is active so it can be cleared. -->
    {#if projects.statuses.length > 1 || projects.statusFilter}
      <div class="px-[18px] pb-2 pt-1">
        <select
          class="status-select h-[28px] w-full rounded-[7px] text-[11.5px] font-medium capitalize {projects.statusFilter
            ? 'is-active'
            : ''}"
          aria-label="Filter by status"
          value={projects.statusFilter ?? ""}
          onchange={(e) => setStatusFilter(e.currentTarget.value)}
        >
          <option value="">All statuses</option>
          {#each filterOptions as status (status)}
            <option value={status}>{status}</option>
          {/each}
        </select>
      </div>
    {/if}
  {/if}

  <!-- Project list -->
  <nav
    class="scroll-thin flex-1 overflow-y-auto px-[10px] pb-[10px] pt-1"
    ondragleave={onListDragLeave}
  >
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
      <div role="list" aria-label="Projects">
      {#each projects.filtered as brief, i (brief.path)}
        {@const sel = brief.path === projects.selectedPath}
        <div
          class="drag-row mx-[6px] border-b py-px last:border-b-0 {dragClass(brief.path)}"
          style="border-color: var(--border-soft);"
          role="listitem"
          draggable="true"
          ondragstart={(e) => onDragStart(e, brief)}
          ondragover={(e) => onDragOver(e, brief)}
          ondrop={(e) => onDrop(e, brief)}
          ondragend={onDragEnd}
        >
          <button
            class="w-full rounded-[9px] px-[10px] py-2 text-left transition-colors {sel
              ? 'bg-[var(--sel)] shadow-[inset_2px_0_0_var(--accent)]'
              : settings.sidebarZebra && i % 2 === 1
                ? 'row-alt'
                : 'hover:bg-[var(--hover)]'}"
            onclick={() => projects.select(brief.path)}
          >
            <div class="flex items-center gap-2">
              <span class="flex-1 truncate text-[13px] text-[var(--fg)] {sel ? 'font-semibold' : 'font-medium'}">{brief.name}</span>
              <StatusPill status={brief.status} />
            </div>
            {#if brief.description}
              <p class="mt-[3px] truncate text-[11.5px] text-[var(--fg3)]">{brief.description}</p>
            {/if}
            <p class="mt-1 text-[10.5px] text-[var(--fg4)]">{relativeTime(brief.lastOpened)}</p>
          </button>
        </div>
      {/each}
      </div>

    {:else if settings.sidebarStyle === "compact"}
      <!-- b) COMPACT — grouped, dense single-line rows -->
      {#each groups as group (group.status)}
        <div
          class="flex items-center gap-[7px] px-2 pb-1 pt-[11px] text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
        >
          {groupLabel(group.status)}
          <span class="ml-auto tabular-nums">{group.items.length}</span>
        </div>
          <div role="list" aria-label={groupLabel(group.status)}>
        {#each group.items as brief, i (brief.path)}
          {@const sel = brief.path === projects.selectedPath}
          <div
            class="drag-row mx-[6px] border-b py-px last:border-b-0 {dragClass(brief.path)}"
            style="border-color: var(--border-soft);"
            role="listitem"
            draggable="true"
            ondragstart={(e) => onDragStart(e, brief)}
            ondragover={(e) => onDragOver(e, brief)}
            ondrop={(e) => onDrop(e, brief)}
            ondragend={onDragEnd}
          >
            <button
              class="flex h-[30px] w-full items-center gap-[9px] rounded-[7px] px-[9px] text-left {sel
                ? 'bg-[var(--sel)] shadow-[inset_2px_0_0_var(--accent)]'
                : settings.sidebarZebra && i % 2 === 1
                  ? 'row-alt'
                  : 'hover:bg-[var(--hover)]'}"
              onclick={() => projects.select(brief.path)}
            >
              <span class="sdot h-[6px] w-[6px]" style="--sc: {statusColor(brief.status)};"></span>
              <span class="flex-1 truncate text-[12.5px] text-[var(--fg)] {sel ? 'font-semibold' : 'font-[450]'}">{brief.name}</span>
              <span class="text-[10px] tabular-nums text-[var(--fg4)]">{relativeTime(brief.lastOpened)}</span>
            </button>
          </div>
        {/each}
          </div>
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
          <div role="list" aria-label={groupLabel(group.status)}>
        {#each group.items as brief (brief.path)}
          {@const pc = statusColor(brief.status)}
          {@const sel = brief.path === projects.selectedPath}
          <div
            class="drag-row is-card {dragClass(brief.path)}"
            role="listitem"
            draggable="true"
            ondragstart={(e) => onDragStart(e, brief)}
            ondragover={(e) => onDragOver(e, brief)}
            ondrop={(e) => onDrop(e, brief)}
            ondragend={onDragEnd}
          >
            <button
              class="mb-[7px] block w-full cursor-pointer rounded-[13px] border-solid bg-[var(--bg)] px-[11px] py-[9px] pl-[12px] text-left transition-[transform,box-shadow] hover:-translate-y-px dark:bg-white/[0.02]"
              style="border-width: {sel ? '1.5px' : '1px'}; border-color: {sel
                ? `color-mix(in srgb, ${pc} 70%, transparent)`
                : 'var(--border)'}; {sel
                ? `box-shadow: 0 2px 10px color-mix(in srgb, ${pc} 22%, transparent);`
                : ''}"
              onclick={() => projects.select(brief.path)}
            >
              <div class="flex items-center gap-[9px]">
                <span
                  class="grid h-[22px] w-[22px] flex-shrink-0 place-items-center"
                  style="border-radius: 8px 11px 9px 12px; background: color-mix(in srgb, {pc} {settings.dark
                    ? '24%'
                    : '14%'}, transparent);"
                >
                  <BrandMark size={12} color={pc} />
                </span>
                <span class="flex-1 truncate text-[12.5px] font-semibold text-[var(--fg)]">{brief.name}</span>
                <span class="text-[10px] text-[var(--fg4)]">{relativeTime(brief.lastOpened)}</span>
              </div>
              {#if brief.description}
                <div
                  class="mt-1 overflow-hidden text-[11px] leading-[1.45] text-[var(--fg3)]"
                  style="display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;"
                >{brief.description}</div>
              {/if}
            </button>
          </div>
        {/each}
          </div>
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
