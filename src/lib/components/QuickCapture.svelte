<script lang="ts">
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { appendCapture } from "$lib/tauri";
  import Icon from "./Icon.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let targetPath = $state<string>("");
  let note = $state("");
  let saving = $state(false);

  // When the modal opens, default the target to the current selection.
  let wasOpen = $state(false);
  $effect(() => {
    if (open && !wasOpen) {
      targetPath = projects.selectedPath ?? projects.briefs[0]?.path ?? "";
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
      toasts.push("Captured 🎉", "success");
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
    class="anim-fade fixed inset-0 z-50 flex items-start justify-center"
    style="background: color-mix(in srgb, var(--fg) 22%, transparent); backdrop-filter: blur(2px); padding-top: 16vh;"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="anim-pop w-[min(92%,520px)] rounded-[16px] border p-4"
      style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
      onkeydown={onKeydown}
    >
      <div class="mb-3 flex items-center justify-between gap-3">
        <h3 class="flex items-center gap-[7px] text-[14px] font-semibold text-[var(--fg)]">
          <Icon name="bolt" size={16} fill={1} /> Quick capture
        </h3>
        <select
          class="rounded-lg border px-2 py-1 text-[12px] text-[var(--fg)] outline-none"
          style="background: var(--input-bg); border-color: var(--border);"
          bind:value={targetPath}
        >
          {#each projects.briefs as b (b.path)}
            <option value={b.path}>{b.name}</option>
          {/each}
        </select>
      </div>

      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="h-[110px] w-full resize-none rounded-[10px] border p-[11px] text-[13px] text-[var(--fg)] outline-none placeholder:text-[var(--fg3)] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
        style="background: var(--input-bg); border-color: var(--border);"
        placeholder="A thought just hit me…"
        bind:value={note}
        autofocus
      ></textarea>

      <div class="mt-3 flex items-center justify-between">
        <span class="text-[11px] text-[var(--fg3)]">⌘/Ctrl+Enter to save · Esc to close</span>
        <button
          class="inline-flex h-[30px] items-center rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={submit}
          disabled={saving}
        >
          {saving ? "Saving…" : "Capture"}
        </button>
      </div>
    </div>
  </div>
{/if}
