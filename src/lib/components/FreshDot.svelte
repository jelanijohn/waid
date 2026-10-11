<script lang="ts">
  // Auto-sync's "new" marker: a 6px accent dot with an optional tabular count.
  // Renders nothing at 0 — and the store returns 0 while the setting is off,
  // so call sites need no extra gate.
  let { count, showCount = true }: { count: number; showCount?: boolean } = $props();
</script>

{#if count > 0}
  <span class="fresh" title="{count} new" aria-label="{count} new">
    <span class="fdot"></span>
    {#if showCount}<span class="fnum">{count}</span>{/if}
  </span>
{/if}

<style>
  .fresh {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    font-weight: 600;
    line-height: 1;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
  }
  .fdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
