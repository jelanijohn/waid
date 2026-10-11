<script lang="ts">
  // One widget-mode roster row: name · status · last opened · chevron, plus the
  // lead brief's Activity line. A button that toggles the brief's leaf; at rest
  // (`still`) it renders as plain content inside the rest panel's one button.
  import type { Brief } from "$lib/types";
  import { statusColor, STATUS_LABEL } from "$lib/status";
  import { shortAgo } from "$lib/time";
  import { activityLine } from "$lib/widget";
  import Icon from "./Icon.svelte";
  import FreshDot from "./FreshDot.svelte";

  let {
    brief,
    lead = false,
    inSession = false,
    open = false,
    side = null,
    still = false,
    fresh = 0,
    onToggle,
  }: {
    brief: Brief;
    lead?: boolean;
    inSession?: boolean;
    open?: boolean;
    /** Side-leaf variant: which side the open leaf is on (chevron points at it). */
    side?: "left" | "right" | null;
    /** Rest state: no button, no chevron. */
    still?: boolean;
    /** Auto-sync: new items across this brief's feeds (0 hides the dot). */
    fresh?: number;
    onToggle?: () => void;
  } = $props();

  let status = $derived((brief.status ?? "").trim());
  let activity = $derived(lead ? activityLine(brief.body) : null);
  let chevron = $derived(
    open ? (side === "left" ? "chevron_left" : side === "right" ? "chevron_right" : "expand_less") : "expand_more",
  );
</script>

{#snippet content()}
  <span class="line">
    <span class="name" class:lead>{brief.name}</span>
    {#if lead && inSession}
      <span class="session">· in session</span>
    {/if}
    <span class="spacer"></span>
    <FreshDot count={fresh} />
    {#if status}
      <span class="status">
        <span class="dot" style="background: {statusColor(status)};"></span>
        {STATUS_LABEL[status.toLowerCase()] ?? status}
      </span>
    {/if}
    <span class="time">{shortAgo(brief.lastOpened)}</span>
    {#if !still}
      <span class="chev" class:open>
        <Icon name={chevron} size={16} />
      </span>
    {/if}
  </span>
  {#if activity}
    <span class="activity" title={activity}>{activity}</span>
  {/if}
{/snippet}

{#if still}
  <div class="row">{@render content()}</div>
{:else}
  <button
    type="button"
    class="row"
    class:sel={open && side !== null}
    aria-expanded={open}
    onclick={onToggle}
  >
    {@render content()}
  </button>
{/if}

<style>
  .row {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    padding: 0;
  }
  button.row {
    cursor: pointer;
  }
  button.row:hover {
    background: var(--hover);
  }
  button.row.sel {
    background: var(--sel);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 4px 12px;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 400;
    color: var(--fg2);
  }
  .name.lead {
    font-weight: 600;
    color: var(--fg);
  }
  .session {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--fg2);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .status {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg2);
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .time {
    flex-shrink: 0;
    min-width: 26px;
    text-align: right;
    font-size: 13px;
    color: var(--fg2);
    font-variant-numeric: tabular-nums;
  }
  .chev {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 20px;
    height: 20px;
    color: var(--fg2);
  }
  .chev.open {
    color: var(--accent);
  }
  .activity {
    display: block;
    padding: 0 12px 6px;
    font-size: 11px;
    line-height: 14px;
    color: var(--fg2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
