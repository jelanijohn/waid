<script lang="ts">
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { appendCapture } from "$lib/tauri";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let targetPath = $state<string>("");
  let note = $state("");
  let saving = $state(false);

  // When the modal opens, default the target to the current selection.
  let wasOpen = $state(false);
  $effect(() => {
    if (open && !wasOpen) {
      targetPath =
        projects.selectedPath ?? projects.briefs[0]?.path ?? "";
      note = "";
    }
    wasOpen = open;
  });

  async function submit() {
    const text = note.trim();
    if (!text || !targetPath) {
      onclose();
      return;
    }
    saving = true;
    try {
      const updated = await appendCapture(targetPath, text);
      projects.upsert(updated);
      toasts.success("Captured");
      note = "";
      onclose();
    } catch (e) {
      toasts.error(`Capture failed: ${e}`);
    } finally {
      saving = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submit();
  }
</script>

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-32"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="w-full max-w-lg rounded-xl bg-white p-4 shadow-2xl dark:bg-slate-800"
      onkeydown={onKeydown}
    >
      <div class="mb-3 flex items-center justify-between">
        <h3 class="text-sm font-semibold text-slate-900 dark:text-slate-100">
          Quick capture
        </h3>
        <select
          class="rounded-md border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 dark:border-slate-600 dark:bg-slate-700 dark:text-slate-200"
          bind:value={targetPath}
        >
          {#each projects.briefs as b (b.path)}
            <option value={b.path}>{b.name}</option>
          {/each}
        </select>
      </div>

      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="h-28 w-full resize-none rounded-lg border border-slate-300 bg-white p-3 text-sm text-slate-800 outline-none focus:ring-2 focus:ring-sky-500 dark:border-slate-600 dark:bg-slate-700 dark:text-slate-100"
        placeholder="A thought just hit me…"
        bind:value={note}
        autofocus
      ></textarea>

      <div class="mt-3 flex items-center justify-between">
        <span class="text-[11px] text-slate-400">⌘/Ctrl+Enter to save · Esc to close</span>
        <button
          class="rounded-md bg-sky-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-sky-700 disabled:opacity-50"
          onclick={submit}
          disabled={saving}
        >
          {saving ? "Saving…" : "Capture"}
        </button>
      </div>
    </div>
  </div>
{/if}
