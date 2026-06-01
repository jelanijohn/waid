<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { relativeTime } from "$lib/time";
  import { createBrief, pickDirectory, setBriefsDir } from "$lib/tauri";
  import { settings, ACCENTS, type SidebarStyle, type Density } from "$lib/stores/settings.svelte";
  import { STATUS_ORDER, STATUS_LABEL, statusColor } from "$lib/status";
  import StatusPill from "./StatusPill.svelte";
  import Icon from "./Icon.svelte";
  import BrandMark from "./BrandMark.svelte";

  let creating = $state(false);
  let newName = $state("");
  let searchEl = $state<HTMLInputElement>();
  let settingsOpen = $state(false);

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
      <button
        class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
        title="View & appearance"
        aria-label="View & appearance"
        onclick={() => (settingsOpen = !settingsOpen)}
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
          class="anim-pop absolute right-0 top-[34px] z-50 w-[232px] rounded-xl border p-3 shadow-[0_16px_40px_rgba(10,20,40,0.28)]"
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
