<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { projects } from "$lib/stores/projects.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import Titlebar from "$lib/components/Titlebar.svelte";
  import ResizeHandles from "$lib/components/ResizeHandles.svelte";
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

<!-- Transparent margin: the native window is borderless + transparent, so this
     padding is just desktop showing through around the rounded panel below and
     gives the panel's shadow room to fall — keeps the app from bleeding into
     the background. -->
<ResizeHandles />

<div class="h-screen overflow-hidden p-[14px]">
  <!-- App panel: the rounded, bordered window shape the whole app lives in.
       overflow-hidden clips the titlebar + content to the corners. -->
  <div
    class="flex h-full flex-col overflow-hidden rounded-[12px] border border-[var(--border)] bg-[var(--bg)] text-[var(--fg)] shadow-[var(--shadow-win)] {settings.density ===
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
</div>

<QuickCapture open={captureOpen} onclose={() => (captureOpen = false)} />
<Toasts />
