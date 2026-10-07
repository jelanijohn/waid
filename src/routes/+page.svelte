<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { projects } from "$lib/stores/projects.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { widget } from "$lib/stores/widget.svelte";
  import { paintsOwnFrame } from "$lib/platform";
  import Titlebar from "$lib/components/Titlebar.svelte";
  import ResizeHandles from "$lib/components/ResizeHandles.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import ProjectDetail from "$lib/components/ProjectDetail.svelte";
  import QuickCapture from "$lib/components/QuickCapture.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import WidgetShell from "$lib/components/WidgetShell.svelte";

  let captureOpen = $state(false);

  // Quick capture: the modal in the dashboard; in widget mode the roster's
  // own capture field takes focus instead.
  function openCapture() {
    if (widget.mode === "widget") widget.requestCaptureFocus();
    else captureOpen = true;
  }

  onMount(() => {
    projects.load();

    // Global hotkey (registered in Rust) surfaces the window + asks us to open
    // quick-capture.
    const unlistenPromise = listen("waid://quick-capture", openCapture);

    // In-window shortcut as well, so capture works without the global binding.
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        openCapture();
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      window.removeEventListener("keydown", onKey);
      unlistenPromise.then((un) => un());
    };
  });
</script>

<!-- Window shape (see lib/platform.ts `paintsOwnFrame`): by default the page
     paints its own rounded, bordered panel inside a 14px transparent gutter
     (desktop showing through the borderless, transparent window). On Windows the
     compositor draws the corners and border itself, so the panel fills the
     window edge-to-edge — a CSS radius there would just stack a second,
     mismatched rounding inside the OS one. -->
{#if widget.mode === "dashboard"}
  <ResizeHandles />
{/if}

<!-- Widget mode keeps the dashboard mounted (unsaved edits, sessions) but
     hidden; the widget is its own fixed layer. -->
<div class:hidden={widget.mode === "widget"}>
<div class="h-screen overflow-hidden {paintsOwnFrame ? 'p-[14px]' : ''}">
  <!-- App panel: the shape the whole app lives in. overflow-hidden clips the
       titlebar + content to the corners when we round them ourselves. -->
  <div
    class="flex h-full flex-col overflow-hidden bg-[var(--bg)] text-[var(--fg)] {paintsOwnFrame
      ? 'rounded-[12px] border border-[var(--border)] shadow-[var(--shadow-win)]'
      : ''} {settings.density === 'compact' ? 'dense' : ''}"
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
</div>

{#if widget.mode === "widget"}
  <WidgetShell />
{/if}

<QuickCapture open={captureOpen} onclose={() => (captureOpen = false)} />
<Toasts />
