<script lang="ts">
  import { onMount } from "svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { relativeTime } from "$lib/time";
  import { createBrief, pickDirectory, setBriefsDir } from "$lib/tauri";
  import { initTheme, setDark } from "$lib/stores/theme";
  import StatusPill from "./StatusPill.svelte";

  let dark = $state(false);
  let creating = $state(false);
  let newName = $state("");
  let searchEl = $state<HTMLInputElement>();

  onMount(() => {
    dark = initTheme();

    // ⌘/Ctrl+F focuses the brief search box.
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchEl?.focus();
        searchEl?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function toggleStatus(status: string) {
    projects.statusFilter = projects.statusFilter === status ? null : status;
  }

  function toggleTheme() {
    dark = !dark;
    setDark(dark);
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
</script>

<aside
  class="flex h-screen w-72 shrink-0 flex-col border-r border-slate-200 bg-slate-50 dark:border-slate-800 dark:bg-slate-900"
>
  <!-- Brand + theme toggle -->
  <header
    class="flex items-center justify-between border-b border-slate-200 px-4 py-3 dark:border-slate-800"
  >
    <div>
      <h1 class="text-sm font-semibold tracking-wide text-slate-900 dark:text-slate-100">
        WAID
      </h1>
      <p class="text-[11px] text-slate-500 dark:text-slate-400">What Am I Doing?</p>
    </div>
    <button
      class="rounded-md p-1.5 text-slate-500 hover:bg-slate-200 dark:text-slate-400 dark:hover:bg-slate-800"
      title="Toggle theme"
      aria-label="Toggle theme"
      onclick={toggleTheme}
    >
      {dark ? "☀" : "☾"}
    </button>
  </header>

  <!-- Search + status filters -->
  {#if projects.briefs.length > 0}
    <div class="border-b border-slate-200 px-3 py-2 dark:border-slate-800">
      <input
        bind:this={searchEl}
        class="w-full rounded-md border border-slate-300 bg-white px-2 py-1.5 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:ring-2 focus:ring-sky-500 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-100"
        placeholder="Search projects… (⌘/Ctrl+F)"
        type="search"
        bind:value={projects.query}
        onkeydown={(e) => {
          if (e.key === "Escape") projects.query = "";
        }}
      />
      {#if projects.statuses.length > 1}
        <div class="mt-2 flex flex-wrap gap-1">
          {#each projects.statuses as status (status)}
            <button
              class="rounded-full px-2 py-0.5 text-xs font-medium capitalize transition-colors {projects.statusFilter ===
              status
                ? 'bg-sky-600 text-white'
                : 'bg-slate-200 text-slate-600 hover:bg-slate-300 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700'}"
              onclick={() => toggleStatus(status)}
            >
              {status}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Project list -->
  <nav class="flex-1 overflow-y-auto p-2">
    {#if projects.loading && projects.briefs.length === 0}
      <p class="px-2 py-3 text-sm text-slate-500">Loading…</p>
    {:else if projects.error}
      <p class="px-2 py-3 text-sm text-rose-600 dark:text-rose-400">{projects.error}</p>
    {:else if projects.briefs.length === 0}
      <p class="px-2 py-3 text-sm text-slate-500">
        No briefs yet. Create one below.
      </p>
    {:else if projects.filtered.length === 0}
      <p class="px-2 py-3 text-sm text-slate-500">No matching projects.</p>
    {/if}

    {#each projects.filtered as brief (brief.path)}
      <button
        class="mb-1 w-full rounded-lg px-3 py-2 text-left transition-colors {brief.path ===
        projects.selectedPath
          ? 'bg-white shadow-sm ring-1 ring-slate-200 dark:bg-slate-800 dark:ring-slate-700'
          : 'hover:bg-slate-100 dark:hover:bg-slate-800/60'}"
        onclick={() => projects.select(brief.path)}
      >
        <div class="flex items-center justify-between gap-2">
          <span class="truncate text-sm font-medium text-slate-900 dark:text-slate-100">
            {brief.name}
          </span>
          <StatusPill status={brief.status} />
        </div>
        {#if brief.description}
          <p class="mt-0.5 truncate text-xs text-slate-500 dark:text-slate-400">
            {brief.description}
          </p>
        {/if}
        <p class="mt-1 text-[11px] text-slate-400 dark:text-slate-500">
          {relativeTime(brief.lastOpened)}
        </p>
      </button>
    {/each}
  </nav>

  <!-- Footer actions -->
  <footer class="border-t border-slate-200 p-2 dark:border-slate-800">
    {#if creating}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="mb-2 w-full rounded-md border border-slate-300 bg-white px-2 py-1.5 text-sm text-slate-900 outline-none focus:ring-2 focus:ring-sky-500 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-100"
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
        class="mb-2 w-full rounded-md bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-700"
        onclick={() => (creating = true)}
      >
        + New project
      </button>
    {/if}

    <div class="flex items-center justify-between gap-2 px-1">
      <span
        class="truncate text-[11px] text-slate-400 dark:text-slate-500"
        title={projects.briefsDir}
      >
        {projects.briefsDir || "…"}
      </span>
      <button
        class="shrink-0 text-[11px] text-sky-600 hover:underline dark:text-sky-400"
        onclick={changeFolder}
      >
        Change
      </button>
    </div>
  </footer>
</aside>
