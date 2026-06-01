<script lang="ts">
  import { statusColor } from "$lib/status";

  let { status }: { status?: string | null } = $props();
</script>

<!-- Tinted pill; `.pill` derives fill + text from the `--sc` status color
     (lighter in dark mode). Unknown statuses fall back to neutral grey. -->
{#if status}
  <span class="pill" style="--sc: {statusColor(status)};">{status}</span>
{/if}
