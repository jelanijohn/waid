<script lang="ts">
  import { morningBriefing } from "$lib/tauri";
  import MarkdownView from "./MarkdownView.svelte";
  import Icon from "./Icon.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let loading = $state(false);
  let error = $state<string | null>(null);
  let text = $state<string | null>(null);

  // Generate once when the modal opens; keep the result if reopened.
  let wasOpen = $state(false);
  $effect(() => {
    if (open && !wasOpen && text === null && !loading) run();
    wasOpen = open;
  });

  async function run() {
    loading = true;
    error = null;
    try {
      text = await morningBriefing();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

{#if open}
  <div
    class="anim-fade fixed inset-[14px] z-50 flex items-start justify-center overflow-hidden rounded-[12px]"
    style="background: color-mix(in srgb, var(--fg) 22%, transparent); backdrop-filter: blur(2px); padding-top: 12vh;"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="anim-pop flex max-h-[76vh] w-[min(92%,560px)] flex-col rounded-[16px] border"
      style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
      onkeydown={onKeydown}
    >
      <div class="flex items-center justify-between gap-3 border-b px-4 py-3" style="border-color: var(--border);">
        <h3 class="flex items-center gap-[7px] text-[14px] font-semibold text-[var(--fg)]">
          <Icon name="wb_sunny" size={16} class="text-[var(--accent)]" /> Morning briefing
        </h3>
        <div class="flex items-center gap-1">
          <button
            class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
            title="Regenerate"
            aria-label="Regenerate briefing"
            onclick={run}
            disabled={loading}
          >
            <Icon name="sync" size={15} class={loading ? "spin" : ""} />
          </button>
          <button
            class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
            aria-label="Close"
            onclick={onclose}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      </div>

      <div class="scroll-thin flex-1 overflow-y-auto px-4 py-3 text-[13px] leading-[1.6] text-[var(--fg-body)]">
        {#if loading && text === null}
          <div class="flex items-center gap-2 py-6 text-[12.5px] text-[var(--fg3)]">
            <Icon name="auto_awesome" size={15} class="spin" /> Gathering work across your briefs…
          </div>
        {:else if error && text === null}
          <p class="py-4 text-[12.5px] text-[var(--status-blocked)]">{error}</p>
        {:else if text}
          <MarkdownView source={text} />
          {#if error}
            <p class="mt-2 text-[11.5px] text-[var(--status-blocked)]">Couldn't refresh: {error}</p>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}
