<script lang="ts">
  // A small help icon that opens an in-app popup explaining how to obtain the
  // credential it sits beside — steps, required scopes, and a link to the
  // provider's docs (opened in the system browser). Topics live in
  // $lib/credentialHelp. Mirrors the modal styling used across the app.
  import Icon from "./Icon.svelte";
  import { credentialHelp } from "$lib/credentialHelp";
  import { openExternal } from "$lib/tauri";

  let { topic, size = 14 }: { topic: string; size?: number } = $props();

  let open = $state(false);

  const help = $derived(credentialHelp[topic]);

  // Render the overlay on <body> so it escapes the surrounding <label> and the
  // parent modal/popover — otherwise their click handlers swallow the close
  // button (clicking it just made the screen flicker).
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      open = false;
    }
  }
</script>

{#if help}
  <button
    type="button"
    class="grid shrink-0 place-items-center rounded-full text-[var(--fg3)] transition-colors hover:text-[var(--accent)]"
    style="height: {size + 6}px; width: {size + 6}px;"
    title="How to get this"
    aria-label="How to get this"
    onclick={(e) => {
      e.preventDefault();
      e.stopPropagation();
      open = true;
    }}
  >
    <Icon name="help" {size} />
  </button>

  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      use:portal
      class="anim-fade fixed inset-[14px] z-[60] flex items-start justify-center overflow-hidden rounded-[12px]"
      style="background: color-mix(in srgb, var(--fg) 22%, transparent); backdrop-filter: blur(2px); padding-top: 12vh;"
      role="presentation"
      tabindex="-1"
      onclick={(e) => {
        if (e.target === e.currentTarget) open = false;
      }}
      onkeydown={onKeydown}
    >
      <div
        class="anim-pop flex max-h-[76vh] w-[min(94%,460px)] flex-col overflow-hidden rounded-[16px] border"
        style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
        role="dialog"
        aria-modal="true"
      >
        <!-- Header -->
        <div class="flex items-center justify-between gap-3 border-b px-4 py-3" style="border-color: var(--border);">
          <h3 class="flex items-center gap-[7px] text-[14px] font-semibold text-[var(--fg)]">
            <Icon name="help" size={16} class="text-[var(--accent)]" />
            {help.title}
          </h3>
          <button
            type="button"
            class="grid h-7 w-7 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
            aria-label="Close"
            onclick={() => (open = false)}
          >
            <Icon name="close" size={16} />
          </button>
        </div>

        <!-- Body -->
        <div class="scroll-thin flex-1 overflow-y-auto px-4 py-3.5">
          <ol class="flex flex-col gap-2 text-[12.5px] leading-[1.55] text-[var(--fg2)]">
            {#each help.steps as step, i (i)}
              <li class="flex gap-2.5">
                <span
                  class="mt-px grid h-[18px] w-[18px] shrink-0 place-items-center rounded-full text-[10.5px] font-semibold text-[var(--fg)]"
                  style="background: var(--chip-bg);"
                >
                  {i + 1}
                </span>
                <span>{step}</span>
              </li>
            {/each}
          </ol>

          {#if help.scopes?.length}
            <div class="mt-3.5">
              <div class="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
                Required scopes
              </div>
              <div class="flex flex-wrap gap-1.5">
                {#each help.scopes as scope (scope)}
                  <code
                    class="rounded-md px-1.5 py-0.5 font-mono text-[11px] text-[var(--fg2)]"
                    style="background: var(--chip-bg);"
                  >
                    {scope}
                  </code>
                {/each}
              </div>
            </div>
          {/if}

          {#if help.note}
            <p class="mt-3.5 text-[11px] leading-[1.5] text-[var(--fg3)]">
              <Icon name="info" size={12} class="-mt-px mr-0.5" />{help.note}
            </p>
          {/if}
        </div>

        <!-- Footer -->
        {#if help.docsUrl}
          <div class="border-t px-4 py-3" style="border-color: var(--border);">
            <button
              type="button"
              class="inline-flex h-[32px] items-center gap-[6px] rounded-lg px-3 text-[12px] font-medium text-[var(--accent)] transition-colors hover:bg-[var(--hover)]"
              onclick={() => help.docsUrl && openExternal(help.docsUrl)}
            >
              Open docs
              <Icon name="open_in_new" size={14} />
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
{/if}
