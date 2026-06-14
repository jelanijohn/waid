<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { projects } from "$lib/stores/projects.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import Titlebar from "$lib/components/Titlebar.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ProjectDetail from "$lib/components/ProjectDetail.svelte";
  import QuickCapture from "$lib/components/QuickCapture.svelte";
  import Toasts from "$lib/components/Toasts.svelte";

  let captureOpen = $state(false);

  onMount(() => {
    projects.load();

    // Global hotkey (registered in Rust) surfaces the window + asks us to open
    // quick-capture.
    const unlistenPromise = listen("waid://quick-capture", () => {
      captureOpen = true;
    });

    // In-window shortcut as well, so capture works without the global binding.
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        captureOpen = true;
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      window.removeEventListener("keydown", onKey);
      unlistenPromise.then((un) => un());
    };
  });
</script>

<div
  class="flex h-screen flex-col overflow-hidden bg-[var(--bg)] text-[var(--fg)] {settings.density ===
  'compact'
    ? 'dense'
    : ''}"
>
  <Titlebar />

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <Sidebar />

    <main class="min-w-0 flex-1">
      {#if projects.selected}
        {#key projects.selected.path}
          <ProjectDetail brief={projects.selected} />
        {/key}
      {:else}
        <div class="flex h-full flex-col items-center justify-center gap-[6px] text-center">
          <p class="text-[15px] font-medium text-[var(--fg3)]">
            {projects.briefs.length ? "Select a project" : "No briefs yet"}
          </p>
          <p class="text-[12.5px] text-[var(--fg3)]">
            Press ⌘/Ctrl+K to quick-capture a thought into any project.
          </p>
        </div>
      {/if}
    </main>
  </div>
</div>

<QuickCapture open={captureOpen} onclose={() => (captureOpen = false)} />
<Toasts />
