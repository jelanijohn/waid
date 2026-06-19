<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief, BriefIntegration, IntegrationFetch, IntegrationItem } from "$lib/types";
  import { integrations } from "$lib/stores/integrations.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { openExternal, appendCapture } from "$lib/tauri";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { PROVIDERS, PROVIDER_ORDER, kindLabel } from "$lib/providers";
  import { relativeTime } from "$lib/time";
  import MarkdownView from "./MarkdownView.svelte";
  import ProviderTile from "./ProviderTile.svelte";
  import Icon from "./Icon.svelte";

  let {
    brief,
    onManage,
    /** Two-column live rail: too narrow for the row CTA → compact stacked card. */
    narrow = false,
    /** Quiet-top layout: render the slim one-line feed strip instead of cards. */
    strip = false,
  }: { brief: Brief; onManage?: () => void; narrow?: boolean; strip?: boolean } = $props();

  // Providers shown in the narrow connect card's overlapped glyph stack.
  const NARROW_PROVIDERS = ["linear", "github", "notion", "gmail"] as const;

  // The AI digest layers over the local rollup; only offered when a synthesis
  // provider (Ollama / Anthropic) is configured.
  let canDigest = $derived(projects.llmProvider !== null);
  let digest = $derived(integrations.digestFor(brief.path));

  // Which feeds have their full item list expanded (keyed connection+kind).
  let expanded = $state<Record<string, boolean>>({});
  function feedKey(ig: BriefIntegration): string {
    return ig.connection + ig.kind + (ig.query ?? "");
  }

  async function runDigest() {
    try {
      await integrations.runDigest(brief.path);
    } catch (e) {
      toasts.error(`Digest failed: ${e}`);
    }
  }

  // Snapshot the digest into the brief's Captures (reuses append_capture).
  async function snapshotDigest() {
    if (!digest?.text) return;
    try {
      const updated = await appendCapture(brief.path, `**Integration digest** — ${digest.text}`);
      projects.upsert(updated);
      integrations.clearDigest(brief.path);
      toasts.success("Snapshotted to Captures");
    } catch (e) {
      toasts.error(`Snapshot failed: ${e}`);
    }
  }

  // Resolve a selector's connection id → its metadata (lives on the brief now).
  function connFor(id: string) {
    return brief.connections.find((c) => c.id === id) ?? null;
  }

  // Mounted under {#key brief.path} (via ProjectDetail), so this runs once per
  // brief: lazily fetch each selector. A failed fetch is swallowed here — the
  // panel renders its own error state. `mind` feeds aren't panel items (they
  // write the ## Mind State body region), so they're skipped here.
  onMount(() => {
    for (const ig of brief.integrations) {
      if (ig.kind === "mind") continue;
      integrations.fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit).catch(() => {});
    }
  });

  // NeuroSkill `mind` feeds sync a deterministic body region instead of caching
  // items — track per-feed sync state and call the dedicated command.
  let mindSyncing = $state<Record<string, boolean>>({});
  async function syncMind(ig: BriefIntegration) {
    const key = feedKey(ig);
    if (mindSyncing[key]) return;
    mindSyncing = { ...mindSyncing, [key]: true };
    try {
      await projects.syncMind(brief.path);
      toasts.success("Mind State updated");
    } catch (e) {
      toasts.error(`Mind State sync failed: ${e}`);
    } finally {
      mindSyncing = { ...mindSyncing, [key]: false };
    }
  }

  // Build the rollup line from the local summary (no LLM).
  function summaryLine(f: IntegrationFetch): string {
    const { total, byStatus, updatedRecently } = f.summary;
    const parts: string[] = [`${total} ${total === 1 ? "item" : "items"}`];
    const statuses = Object.entries(byStatus);
    if (statuses.length) parts.push(statuses.map(([s, n]) => `${n} ${s}`).join(", "));
    if (updatedRecently) parts.push(`${updatedRecently} updated this week`);
    return parts.join(" · ");
  }

  // Map an item status to a status-dot color (rock palette).
  function dotColor(status?: string | null): string {
    if (!status) return "var(--fg4)";
    const s = status.toLowerCase();
    if (/(done|closed|merged|complete|resolved|approved|read)/.test(s)) return "var(--status-active)";
    if (/(progress|started|review|doing|mention|todo)/.test(s)) return "var(--status-paused)";
    if (/(block|fail|overdue|urgent|broke|unread)/.test(s)) return "var(--status-blocked)";
    return "var(--fg4)";
  }

  async function refresh(ig: BriefIntegration) {
    try {
      await integrations.fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit, true);
    } catch (e) {
      toasts.error(`Fetch failed: ${e}`);
    }
  }

  async function open(item: IntegrationItem) {
    if (!item.url) return;
    try {
      await openExternal(item.url);
    } catch (e) {
      toasts.error(`Could not open link (${e}).`);
    }
  }
</script>

{#if brief.integrations.length}
  {#if strip}
    <!-- Quiet-top layout: a slim one-line rollup, one pill per connected feed. -->
    <div class="flex items-center gap-3">
      <div class="flex flex-wrap items-center gap-[10px]">
        {#each brief.integrations as ig (feedKey(ig))}
          {@const conn = connFor(ig.connection)}
          {@const entry = integrations.get(brief.path, ig.connection, ig.kind, ig.query)}
          <button
            class="inline-flex items-center gap-2 rounded-full border bg-[var(--bg)] py-[5px] pl-[6px] pr-[10px] transition-colors hover:bg-[var(--hover)]"
            style="border-color: var(--border);"
            onclick={onManage}
          >
            {#if conn}
              <ProviderTile provider={conn.provider} size={20} />
            {:else}
              <span class="grid h-[20px] w-[20px] place-items-center rounded-[7px] text-[var(--fg3)]" style="background: var(--chip-bg);">
                <Icon name="link_off" size={12} />
              </span>
            {/if}
            <span class="text-[12px] text-[var(--fg-body)]">
              {#if ig.kind === "mind"}
                mind state
              {:else}
                <strong class="font-semibold text-[var(--fg)] tabular-nums">{entry?.data?.items.length ?? 0}</strong>
                {kindLabel(ig.kind, conn?.provider).toLowerCase()}
              {/if}
            </span>
          </button>
        {/each}
      </div>
      {#if onManage}
        <button class="btn-quiet ml-auto" style="font-size: 11.5px;" onclick={onManage}>View feeds</button>
      {/if}
    </div>
  {:else}
    <div class="flex flex-col gap-[10px]">
      {#if canDigest}
        <!-- AI digest: natural-language layer over the local rollups -->
        <div class="rounded-[11px] border" style="border-color: var(--border);">
          <div class="flex items-center gap-2 px-[13px] py-[9px]">
            <Icon name="auto_awesome" size={15} class="text-[var(--accent)]" />
            <span class="text-[12.5px] font-semibold text-[var(--fg)]">AI digest</span>
            {#if digest?.text}
              <button class="btn btn-sm ml-auto" title="Append this digest to the brief's Captures" onclick={snapshotDigest}>
                <Icon name="bookmark_add" size={13} /> Snapshot
              </button>
              <button
                class="btn-icon"
                style="height: 26px; width: 26px;"
                title="Regenerate"
                aria-label="Regenerate digest"
                onclick={runDigest}
                disabled={digest?.loading}
              >
                <Icon name="sync" size={13} class={digest?.loading ? "spin" : ""} />
              </button>
            {:else}
              <button class="btn btn-sm ml-auto" onclick={runDigest} disabled={digest?.loading}>
                <Icon name="auto_awesome" size={13} class={digest?.loading ? "spin" : ""} />
                {digest?.loading ? "Thinking…" : "Generate"}
              </button>
            {/if}
          </div>
          {#if digest?.text}
            <div class="border-t px-[13px] py-2 text-[12.5px] leading-[1.55] text-[var(--fg-body)]" style="border-color: var(--border-soft);">
              <MarkdownView source={digest.text} />
            </div>
          {/if}
          {#if digest?.error}
            <div class="border-t px-[13px] py-2 text-[11.5px] text-[var(--status-blocked)]" style="border-color: var(--border-soft);">
              {digest.error}
            </div>
          {/if}
        </div>
      {/if}

      {#each brief.integrations as ig (feedKey(ig))}
        {@const conn = connFor(ig.connection)}
        {@const entry = integrations.get(brief.path, ig.connection, ig.kind, ig.query)}
        <div class="overflow-hidden rounded-[11px] border" style="border-color: var(--border);">
          {#if !conn}
            <!-- Connection isn't set up anymore — quiet reconnect banner. -->
            <div class="flex items-center gap-[10px] px-3 py-[10px]">
              <span class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-[7px] text-[var(--fg3)]" style="background: var(--chip-bg);">
                <Icon name="link_off" size={15} />
              </span>
              <div class="min-w-0 flex-1">
                <div class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{ig.connection}</div>
                <div class="text-[11px] text-[var(--fg3)]">Account isn't connected anymore</div>
              </div>
              {#if onManage}
                <button class="btn btn-sm shrink-0" onclick={onManage}>Reconnect</button>
              {/if}
            </div>
          {:else if ig.kind === "mind"}
            <!-- NeuroSkill mind state: not panel items. A slim status line whose
                 refresh regenerates the deterministic ## Mind State body region. -->
            <div class="flex items-center gap-[10px] px-3 py-[9px]">
              <ProviderTile provider={conn.provider} size={26} />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-[7px]">
                  <span class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{conn.label}</span>
                  <span class="shrink-0 rounded-[5px] bg-[var(--chip-bg)] px-[6px] py-px text-[9.5px] font-semibold uppercase tracking-[0.04em] text-[var(--fg3)]">
                    {kindLabel(ig.kind, conn.provider)}
                  </span>
                </div>
                <div class="mt-px truncate text-[11px] text-[var(--fg3)]">
                  {brief.body.includes("waid:mind:start") ? "Synced to ## Mind State" : "Not synced yet"}
                  · {ig.query ?? "14d"}
                </div>
              </div>
              <button
                class="btn-icon shrink-0"
                style="height: 26px; width: 26px;"
                title="Regenerate ## Mind State"
                aria-label="Regenerate Mind State for {conn.label}"
                onclick={() => syncMind(ig)}
                disabled={mindSyncing[feedKey(ig)]}
              >
                <Icon name="sync" size={14} class={mindSyncing[feedKey(ig)] ? "spin" : ""} />
              </button>
            </div>
          {:else}
            <!-- Feed header -->
            <div class="flex items-center gap-[10px] px-3 py-[9px]">
              <ProviderTile provider={conn.provider} size={26} />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-[7px]">
                  <span class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{conn.label}</span>
                  <span class="shrink-0 rounded-[5px] bg-[var(--chip-bg)] px-[6px] py-px text-[9.5px] font-semibold uppercase tracking-[0.04em] text-[var(--fg3)]">
                    {kindLabel(ig.kind, conn.provider)}
                  </span>
                </div>
                {#if entry?.data}
                  <div class="mt-px truncate text-[11px] text-[var(--fg3)]">{summaryLine(entry.data)}</div>
                {/if}
              </div>
              <button
                class="btn-icon shrink-0"
                style="height: 26px; width: 26px;"
                title="Refresh"
                aria-label="Refresh {conn.label}"
                onclick={() => refresh(ig)}
                disabled={entry?.loading}
              >
                <Icon name="sync" size={14} class={entry?.loading ? "spin" : ""} />
              </button>
            </div>

            <!-- Feed body: error / loading / empty / items -->
            {#if entry?.error && !entry.data}
              <div class="border-t px-3 py-[10px] text-[12px] text-[var(--status-blocked)]" style="border-color: var(--border);">
                {entry.error}
              </div>
            {:else if entry?.loading && !entry.data}
              <div class="flex items-center gap-2 border-t px-3 py-[10px] text-[12px] text-[var(--fg3)]" style="border-color: var(--border);">
                <Icon name="sync" size={14} class="spin" /> Loading…
              </div>
            {:else if entry?.data && entry.data.items.length === 0}
              <div class="border-t px-3 py-[10px] text-[12px] text-[var(--fg3)]" style="border-color: var(--border);">
                Nothing assigned right now. 🎉
              </div>
            {:else if entry?.data}
              {@const all = entry.data.items}
              {@const isOpen = expanded[feedKey(ig)] ?? false}
              {@const shown = isOpen ? all : all.slice(0, 3)}
              <ul class="border-t" style="border-color: var(--border);">
                {#each shown as item (item.id)}
                  <!-- `group` so the row reveals its excerpt on hover OR keyboard focus -->
                  <li class="group border-t first:border-t-0" style="border-color: var(--border-soft);">
                    <button
                      class="flex w-full items-center gap-[9px] px-3 py-[8px] text-left transition-colors group-hover:bg-[var(--hover)]"
                      onclick={() => open(item)}
                    >
                      <span class="h-[6px] w-[6px] shrink-0 rounded-full" style="background: {dotColor(item.status)};"></span>
                      <!-- Who initiated it: sender (Slack username) / assignee / From -->
                      {#if item.assignee}
                        <span class="max-w-[40%] shrink-0 truncate text-[12px] font-semibold text-[var(--fg)]">{item.assignee}</span>
                      {/if}
                      <span class="min-w-0 flex-1 truncate text-[12px] text-[var(--fg-body)]">{item.title}</span>
                      {#if item.updatedAt}
                        <span class="shrink-0 text-[10px] tabular-nums text-[var(--fg4)]">{relativeTime(item.updatedAt)}</span>
                      {/if}
                      <Icon name="north_east" size={11} class="shrink-0 text-[var(--fg4)]" />
                    </button>
                    <!-- Larger excerpt — slides open on hover/focus, stays collapsed otherwise -->
                    {#if item.meta?.snippet || item.meta?.channel}
                      <div
                        class="max-h-0 overflow-hidden opacity-0 transition-[max-height,opacity] duration-200 ease-out group-hover:max-h-[240px] group-hover:opacity-100 group-focus-within:max-h-[240px] group-focus-within:opacity-100"
                      >
                        <div class="flex items-center gap-[7px] px-3 pl-[27px] pt-px">
                          {#if item.assignee}
                            <span class="text-[11px] font-semibold text-[var(--fg2)]">{item.assignee}</span>
                          {/if}
                          {#if item.meta?.channel}
                            <span class="rounded-[5px] bg-[var(--chip-bg)] px-[6px] py-px text-[10px] font-semibold text-[var(--fg3)]">{item.meta.channel}</span>
                          {/if}
                          {#if item.updatedAt}
                            <span class="ml-auto text-[10px] tabular-nums text-[var(--fg4)]">{relativeTime(item.updatedAt)}</span>
                          {/if}
                        </div>
                        <p class="px-3 pb-[10px] pl-[27px] pt-[3px] text-[11.5px] leading-[1.5] text-[var(--fg2)] [text-wrap:pretty]">
                          {item.meta?.snippet ?? item.title}
                        </p>
                      </div>
                    {/if}
                  </li>
                {/each}
              </ul>
              {#if all.length > 3}
                <button
                  class="w-full border-t px-3 py-[7px] text-left text-[11px] font-medium text-[var(--accent)] hover:underline"
                  style="border-color: var(--border-soft);"
                  onclick={() => (expanded = { ...expanded, [feedKey(ig)]: !isOpen })}
                >
                  {isOpen ? "Show less" : `+ ${all.length - 3} more in ${PROVIDERS[conn.provider].label}`}
                </button>
              {/if}
              {#if entry.error}
                <!-- Stale data shown above, but the latest refresh failed. -->
                <div class="border-t px-3 py-[7px] text-[11px] text-[var(--status-blocked)]" style="border-color: var(--border-soft);">
                  Couldn't refresh: {entry.error}
                </div>
              {/if}
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{:else if narrow}
  <!-- Empty state, two-column rail: a tidy compact stacked card. -->
  <div class="flex flex-col gap-[10px] rounded-[12px] border bg-[var(--bg)] p-[14px]" style="border-color: var(--border);">
    <div class="flex">
      {#each NARROW_PROVIDERS as p, i (p)}
        <span style="margin-left: {i ? -7 : 0}px; box-shadow: 0 0 0 2px var(--bg); border-radius: 7px;">
          <ProviderTile provider={p} size={24} />
        </span>
      {/each}
    </div>
    <div>
      <div class="text-[12.5px] font-semibold text-[var(--fg)]">Connect a tool</div>
      <div class="mt-[2px] text-[11.5px] leading-[1.45] text-[var(--fg3)]">Pull open tasks &amp; notifications into this brief.</div>
    </div>
    <button class="btn btn-primary btn-sm w-full justify-center" onclick={onManage}>Connect</button>
  </div>
{:else}
  <!-- Empty state, full width: a slim one-line row. -->
  <button
    class="flex w-full items-center gap-3 rounded-[11px] border bg-[var(--bg)] px-[13px] py-[10px] text-left transition-colors hover:bg-[var(--hover)]"
    style="border-color: var(--border);"
    onclick={onManage}
  >
    <span class="flex">
      {#each PROVIDER_ORDER as p, i (p)}
        <span style="margin-left: {i ? -6 : 0}px; box-shadow: 0 0 0 2px var(--bg); border-radius: 7px;">
          <ProviderTile provider={p} size={22} />
        </span>
      {/each}
    </span>
    <span class="flex-1 text-[12.5px] text-[var(--fg2)]">Pull live tasks &amp; notifications into this brief</span>
    <span class="inline-flex items-center gap-1 text-[12.5px] font-medium text-[var(--accent)]">
      Connect <Icon name="arrow_forward" size={14} />
    </span>
  </button>
{/if}
