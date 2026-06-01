<script lang="ts">
  let { status }: { status?: string | null } = $props();

  const styles: Record<string, string> = {
    active: "bg-emerald-100 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300",
    paused: "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300",
    blocked: "bg-rose-100 text-rose-700 dark:bg-rose-500/15 dark:text-rose-300",
    archived: "bg-slate-200 text-slate-600 dark:bg-slate-700 dark:text-slate-300",
  };

  // Unknown statuses render as a neutral pill (we don't enforce the set).
  let cls = $derived(
    styles[(status ?? "").toLowerCase()] ??
      "bg-slate-200 text-slate-600 dark:bg-slate-700 dark:text-slate-300",
  );
</script>

{#if status}
  <span class="inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium {cls}">
    {status}
  </span>
{/if}
