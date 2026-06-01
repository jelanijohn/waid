<script lang="ts">
  import { toasts } from "$lib/stores/toasts.svelte";
  import Icon from "./Icon.svelte";

  // The design defines two surfaces: success (green) and error (red). Info maps
  // onto the success surface so neutral notices stay legible.
  function isError(kind: string): boolean {
    return kind === "error";
  }
</script>

<div class="pointer-events-none fixed bottom-[18px] left-1/2 z-[60] flex -translate-x-1/2 flex-col items-center gap-2">
  {#each toasts.items as toast (toast.id)}
    <button
      class="anim-toast pointer-events-auto flex max-w-[90vw] items-center gap-2 rounded-[10px] px-[14px] py-[9px] text-[12.5px] font-medium shadow-[0_8px_24px_rgba(10,20,40,0.25)]"
      style={isError(toast.kind)
        ? "background: #7F2C2C; color: #FBE0E0;"
        : "background: #1B5E45; color: #DDF3E8;"}
      onclick={() => toasts.dismiss(toast.id)}
    >
      <Icon name={isError(toast.kind) ? "error" : "check_circle"} size={15} fill={1} />
      {toast.message}
    </button>
  {/each}
</div>
