<script lang="ts">
  import { toasts } from "$lib/stores/toasts.svelte";

  const styles: Record<string, string> = {
    success: "bg-emerald-600 text-white",
    error: "bg-rose-600 text-white",
    info: "bg-slate-800 text-white dark:bg-slate-700",
  };
</script>

<div class="pointer-events-none fixed bottom-4 right-4 z-[60] flex flex-col gap-2">
  {#each toasts.items as toast (toast.id)}
    <button
      class="pointer-events-auto max-w-sm rounded-lg px-4 py-2 text-sm shadow-lg {styles[
        toast.kind
      ]}"
      onclick={() => toasts.dismiss(toast.id)}
    >
      {toast.message}
    </button>
  {/each}
</div>
