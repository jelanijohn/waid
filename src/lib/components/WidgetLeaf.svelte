<script lang="ts">
  // A widget-mode leaf: one brief's Current State, a count per feed, its launch
  // chips, session control and "Open brief". `accordion` renders as a band
  // under the row; `side` fills the panel beside the roster. Peeking never
  // selects the brief (no last_opened write); launching does.
  import { onMount, untrack } from "svelte";
  import type { Brief, BriefIntegration, IntegrationItem } from "$lib/types";
  import { projects, isSyncableBrief } from "$lib/stores/projects.svelte";
  import { integrations, type Highlights } from "$lib/stores/integrations.svelte";
  import { session } from "$lib/stores/session.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { widget } from "$lib/stores/widget.svelte";
  import { launchLink, launchWebhook, wslHint } from "$lib/actions";
  import { openExternal } from "$lib/tauri";
  import { kindLabel } from "$lib/providers";
  import { statusColor } from "$lib/status";
  import { relativeTime } from "$lib/time";
  import { currentStateExcerpt, feedCountText } from "$lib/widget";
  import ProviderTile from "./ProviderTile.svelte";
  import Icon from "./Icon.svelte";

  let { brief, variant }: { brief: Brief; variant: "accordion" | "side" } = $props();

  const side = $derived(variant === "side");
  let excerpt = $derived(currentStateExcerpt(brief.body));
  let summary = $derived(excerpt ?? brief.description ?? null);
  let hasNeuro = $derived(brief.connections.some((c) => c.provider === "neuroskill"));
  let inSession = $derived(session.isActive(brief.path));
  let refreshing = $state(false);

  function connFor(id: string) {
    return brief.connections.find((c) => c.id === id) ?? null;
  }
  function entryFor(ig: BriefIntegration) {
    return integrations.get(brief.path, ig.connection, ig.kind, ig.query);
  }
  function feedKey(ig: BriefIntegration): string {
    return ig.connection + ig.kind + (ig.query ?? "");
  }

  // Auto-sync: a leaf is only mounted while open, so the brief's new items are
  // in front of the user — mark them seen and keep them highlighted here.
  let highlight = $state<Highlights>(new Map());
  $effect(() => {
    if (integrations.freshCount(brief.path) === 0) return;
    untrack(() => {
      highlight = integrations.takeFresh(brief.path, highlight);
    });
  });
  function isNew(ig: BriefIntegration, item: IntegrationItem): boolean {
    return integrations.isHighlighted(highlight, brief.path, ig, item);
  }
  function newCount(ig: BriefIntegration): number {
    if (!highlight.size) return 0;
    return entryFor(ig)?.data?.items.filter((it) => isNew(ig, it)).length ?? 0;
  }

  /** Oldest fetch time across the feeds that have data. */
  let asOf = $derived.by(() => {
    let oldest: string | null = null;
    for (const ig of brief.integrations) {
      if (ig.kind === "mind") continue;
      const at = entryFor(ig)?.data?.fetchedAt;
      if (at && (!oldest || new Date(at) < new Date(oldest))) oldest = at;
    }
    return oldest;
  });

  // Same lazy fetch IntegrationPanel does on open: cached results come back
  // without a network call. `mind` feeds aren't items (they write a body region).
  onMount(() => {
    for (const ig of brief.integrations) {
      if (ig.kind === "mind") continue;
      integrations.fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit).catch(() => {});
    }
  });

  /** Force-fetch every feed, plus the deterministic Activity sync when the
   *  brief has something to sync. No synthesis / Mind State from here. */
  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    try {
      await Promise.all(
        brief.integrations
          .filter((ig) => ig.kind !== "mind")
          .map((ig) =>
            integrations
              .fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit, true)
              .catch((e) => toasts.error(`Fetch failed: ${e}`)),
          ),
      );
      if (isSyncableBrief(brief)) {
        try {
          await projects.sync(brief.path);
        } catch (e) {
          toasts.error(`Refresh failed: ${e}`);
        }
      }
    } finally {
      refreshing = false;
    }
  }

  /** Launching is "this is what I'm doing now", so it selects the brief first. */
  async function launch(action: (b: Brief) => Promise<void>) {
    if (projects.selectedPath !== brief.path) await projects.select(brief.path);
    await action(projects.briefs.find((b) => b.path === brief.path) ?? brief);
  }

  async function openItem(item: IntegrationItem) {
    if (!item.url) return;
    try {
      await openExternal(item.url);
    } catch (e) {
      toasts.error(`Could not open link (${e}).${wslHint()}`);
    }
  }
</script>

{#snippet openBrief()}
  <button type="button" class="open" onclick={() => widget.exit(brief.path)}>
    Open brief <Icon name="north_east" size={12} />
  </button>
{/snippet}

<div class="leaf {variant} {side ? '' : 'scroll-thin'}">
  {#if side}
    <div class="block">
      <div class="title">
        <span class="tdot" style="background: {statusColor(brief.status)};"></span>
        <span class="tname">{brief.name}</span>
        <span class="flex-1"></span>
        {@render openBrief()}
      </div>
      {#if brief.description}
        <p class="desc">{brief.description}</p>
      {/if}
    </div>
  {/if}

  {#if summary && !(side && !excerpt)}
    <div class="block">
      <div class="label">{excerpt ? "Current state" : "About"}</div>
      <p class="summary" style="-webkit-line-clamp: {side ? 8 : 4};">{summary}</p>
    </div>
  {/if}

  {#if brief.integrations.length}
    <div class="block">
      <div class="label-row">
        <span class="label">Live state</span>
        <span class="flex-1"></span>
        {#if asOf}<span class="asof">as of {relativeTime(asOf)}</span>{/if}
        <button
          type="button"
          class="refresh"
          title="Refresh feeds"
          aria-label="Refresh feeds"
          onclick={refresh}
          disabled={refreshing}
        >
          <Icon name="sync" size={14} class={refreshing ? "spin" : ""} />
        </button>
      </div>
      {#each brief.integrations as ig (feedKey(ig))}
        {@const conn = connFor(ig.connection)}
        {@const entry = entryFor(ig)}
        <div class="feed">
          {#if conn}<ProviderTile provider={conn.provider} size={16} />{/if}
          <span class="flabel">{kindLabel(ig.kind, conn?.provider)}</span>
          <span class="flex-1"></span>
          <span class="count" title={entry?.error && !entry.data ? entry.error : undefined}>
            {feedCountText(ig, entry)}{#if newCount(ig)}<span class="new"> · {newCount(ig)} new</span>{/if}
          </span>
        </div>
        {#if side && entry?.data}
          {#each entry.data.items.slice(0, 2) as item (item.id)}
            <button type="button" class="item" title={item.title} onclick={() => openItem(item)}>
              {#if isNew(ig, item)}<span class="idot" aria-label="new"></span>{/if}
              <span class="ititle">{item.title}</span>
              {#if item.updatedAt}<span class="itime">{relativeTime(item.updatedAt)}</span>{/if}
            </button>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}

  {#if brief.links.length || brief.webhooks.length}
    <div class="chips">
      {#each brief.links as link (link.url + link.label)}
        <button type="button" class="chip-l" title={link.url} onclick={() => launch((b) => launchLink(b, link.url))}>
          {link.label || link.url}
          <Icon name="north_east" size={12} class="text-[var(--fg2)]" />
        </button>
      {/each}
      {#each brief.webhooks as hook (hook.url + hook.label)}
        <button
          type="button"
          class="chip-l hook"
          title={`${hook.method} ${hook.url}`}
          onclick={() => launch((b) => launchWebhook(b, hook))}
        >
          <Icon name="bolt" size={12} fill={1} />
          {hook.label || hook.url}
          <span class="method">{hook.method}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if hasNeuro || !side}
    <div class="actions">
      {#if hasNeuro}
        {#if inSession}
          <button type="button" class="sess" onclick={() => session.end()}>
            <Icon name="stop_circle" size={14} fill={1} /> End session
          </button>
        {:else}
          <button type="button" class="sess" onclick={() => launch((b) => session.start(b))}>
            <Icon name="neurology" size={14} /> Start session
          </button>
        {/if}
      {/if}
      <span class="flex-1"></span>
      {#if !side}{@render openBrief()}{/if}
    </div>
  {/if}
</div>

<style>
  .leaf {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .leaf.accordion {
    padding: 12px;
    background: var(--row-alt);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    max-height: 320px;
    overflow-y: auto;
    /* Own compositing layer: WebKitGTK leaves repaint trails in a scroller
       under an opacity-reduced ancestor (WidgetShell's widget opacity). */
    transform: translateZ(0);
  }
  .block {
    display: flex;
    flex-direction: column;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .tname {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 600;
    color: var(--fg);
  }
  .desc {
    margin-top: 6px;
    font-size: 12px;
    line-height: 17px;
    color: var(--fg2);
  }
  .label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg2);
  }
  .label-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .block > .label {
    margin-bottom: 6px;
  }
  .summary {
    font-size: 12px;
    line-height: 17px;
    color: var(--fg-body);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .asof {
    font-size: 11px;
    color: var(--fg2);
  }
  .refresh {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--fg2);
    cursor: pointer;
  }
  .refresh:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .refresh:disabled {
    cursor: default;
  }
  .feed {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    font-size: 12px;
  }
  .flabel {
    color: var(--fg-body);
    white-space: nowrap;
  }
  .side .flabel {
    font-weight: 500;
    color: var(--fg);
  }
  .count {
    color: var(--fg2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 22px;
    padding-left: 24px;
    border: none;
    background: transparent;
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
  }
  .item:hover {
    background: var(--hover);
  }
  .new {
    color: var(--accent);
    font-weight: 600;
  }
  /* Sits in the row's left padding so the title doesn't shift. */
  .idot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    margin-left: -14px;
    border-radius: 50%;
    background: var(--accent);
  }
  .ititle {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-body);
  }
  .itime {
    flex-shrink: 0;
    color: var(--fg2);
    font-variant-numeric: tabular-nums;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip-l {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--bg);
    font-size: 12px;
    color: var(--fg-body);
    cursor: pointer;
    transition: background 0.14s;
  }
  .chip-l:hover {
    background: var(--hover);
  }
  .chip-l.hook {
    border-color: var(--hook-bd);
    background: var(--hook-bg);
    color: var(--hook-fg);
  }
  .chip-l.hook:hover {
    filter: brightness(1.04);
  }
  .method {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
  }
  .actions {
    display: flex;
    align-items: center;
  }
  .sess {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: none;
    background: transparent;
    font-size: 12px;
    color: var(--fg-body);
    cursor: pointer;
    padding: 0;
  }
  .sess:hover {
    color: var(--fg);
  }
  .open {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    border: none;
    background: transparent;
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
  }
  .open:hover {
    text-decoration: underline;
  }
</style>
