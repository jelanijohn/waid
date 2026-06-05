<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief, BriefIntegration, IntegrationFetch, IntegrationItem, Provider } from "$lib/types";
  import { integrations } from "$lib/stores/integrations.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { openExternal, appendCapture } from "$lib/tauri";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { PROVIDERS, PROVIDER_ORDER, kindLabel } from "$lib/providers";
  import { relativeTime } from "$lib/time";
  import MarkdownView from "./MarkdownView.svelte";
  import ProviderTile from "./ProviderTile.svelte";
  import Icon from "./Icon.svelte";

  let { brief, onManage }: { brief: Brief; onManage?: () => void } = $props();

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
  // panel renders its own error state.
  onMount(() => {
    for (const ig of brief.integrations) {
      integrations.fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit).catch(() => {});
    }
  });

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
    if (!status) return "var(--fg3)";
    const s = status.toLowerCase();
    if (/(done|closed|merged|complete|resolved|approved)/.test(s)) return "var(--status-active)";
    if (/(progress|started|review|doing|mention)/.test(s)) return "var(--status-paused)";
    if (/(block|fail|overdue|urgent|broke)/.test(s)) return "var(--status-blocked)";
    return "var(--fg3)";
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
  <section class="mb-[26px] flex flex-col gap-[10px]">
    {#if canDigest}
      <!-- AI digest: natural-language layer over the local rollups -->
      <div class="rounded-[11px] border" style="border-color: var(--border);">
        <div class="flex items-center gap-2 px-[13px] py-[9px]">
          <Icon name="auto_awesome" size={15} class="text-[var(--accent)]" />
          <span class="text-[12.5px] font-semibold text-[var(--fg)]">AI digest</span>
          {#if digest?.text}
            <button
              class="ml-auto inline-flex h-[26px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-2.5 text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
              style="border-color: var(--border);"
              title="Append this digest to the brief's Captures"
              onclick={snapshotDigest}
            >
              <Icon name="bookmark_add" size={13} /> Snapshot
            </button>
            <button
              class="grid h-[26px] w-[26px] place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
              title="Regenerate"
              aria-label="Regenerate digest"
              onclick={runDigest}
              disabled={digest?.loading}
            >
              <Icon name="sync" size={13} class={digest?.loading ? "spin" : ""} />
            </button>
          {:else}
            <button
              class="ml-auto inline-flex h-[26px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-2.5 text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
              style="border-color: var(--border);"
              onclick={runDigest}
              disabled={digest?.loading}
            >
              <Icon name="auto_awesome" size={13} class={digest?.loading ? "spin" : ""} />
              {digest?.loading ? "Thinking…" : "Generate"}
            </button>
          {/if}
        </div>
        {#if digest?.text}
          <div class="border-t px-[13px] py-2 text-[12.5px] leading-[1.55] text-[var(--fg-body)]" style="border-color: var(--border);">
            <MarkdownView source={digest.text} />
          </div>
        {/if}
        {#if digest?.error}
          <div class="border-t px-[13px] py-2 text-[11.5px] text-[var(--status-blocked)]" style="border-color: var(--border);">
            {digest.error}
          </div>
        {/if}
      </div>
    {/if}

    {#each brief.integrations as ig (feedKey(ig))}
      {@const conn = connFor(ig.connection)}
      {@const entry = integrations.get(brief.path, ig.connection, ig.kind, ig.query)}
      <div class="overflow-hidden rounded-[12px] border" style="border-color: var(--border);">
        {#if !conn}
          <!-- Redesigned "connection isn't set up" — amber reconnect banner. -->
          <div class="flex items-center gap-[10px] px-3 py-[10px]">
            <span
              class="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-lg text-[var(--fg3)]"
              style="background: var(--chip-bg);"
            >
              <Icon name="link_off" size={16} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{ig.connection}</div>
              <div class="text-[11px] text-[var(--fg3)]">Account isn't connected anymore</div>
            </div>
          </div>
          <div
            class="flex items-center gap-2 border-t px-3 py-[9px]"
            style="border-color: var(--border); background: color-mix(in srgb, var(--status-paused) 12%, transparent);"
          >
            <Icon name="warning" size={15} fill={1} class="text-[var(--hook-fg)]" />
            <span class="flex-1 text-[11.5px] text-[var(--fg2)]">This feed lost its account. Reconnect to keep pulling data.</span>
            {#if onManage}
              <button
                class="inline-flex h-[26px] shrink-0 items-center rounded-[7px] bg-[var(--accent)] px-2.5 text-[11.5px] font-medium text-white transition-[filter] hover:brightness-[1.06]"
                onclick={onManage}
              >
                Reconnect
              </button>
            {/if}
          </div>
        {:else}
          <!-- Feed header -->
          <div class="flex items-center gap-[10px] px-3 py-[10px]">
            <ProviderTile provider={conn.provider} size={30} />
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
              class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
              title="Refresh"
              aria-label="Refresh {conn.label}"
              onclick={() => refresh(ig)}
              disabled={entry?.loading}
            >
              <Icon name="sync" size={15} class={entry?.loading ? "spin" : ""} />
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
                <li class="border-t first:border-t-0" style="border-color: color-mix(in srgb, var(--border) 60%, transparent);">
                  <button
                    class="flex w-full items-center gap-[9px] px-3 py-[8px] text-left transition-colors hover:bg-[var(--hover)]"
                    onclick={() => open(item)}
                  >
                    <span class="h-[7px] w-[7px] shrink-0 rounded-full" style="background: {dotColor(item.status)};"></span>
                    <span class="min-w-0 flex-1 truncate text-[12px] text-[var(--fg-body)]">{item.title}</span>
                    {#if item.status}
                      <span class="shrink-0 text-[10px] text-[var(--fg3)]">{item.status}</span>
                    {/if}
                    <Icon name="north_east" size={11} class="shrink-0 text-[var(--fg3)] opacity-60" />
                  </button>
                </li>
              {/each}
            </ul>
            {#if all.length > 3}
              <button
                class="w-full border-t px-3 py-[7px] text-left text-[11px] font-medium text-[var(--accent)] hover:underline"
                style="border-color: color-mix(in srgb, var(--border) 60%, transparent);"
                onclick={() => (expanded = { ...expanded, [feedKey(ig)]: !isOpen })}
              >
                {isOpen ? "Show less" : `+ ${all.length - 3} more in ${PROVIDERS[conn.provider].label}`}
              </button>
            {/if}
            {#if entry.error}
              <!-- Stale data shown above, but the latest refresh failed. -->
              <div class="border-t px-3 py-[7px] text-[11px] text-[var(--status-blocked)]" style="border-color: var(--border);">
                Couldn't refresh: {entry.error}
              </div>
            {/if}
          {/if}
        {/if}
      </div>
    {/each}
  </section>
{:else}
  <!-- Empty state — invite the user to pull real work into the brief. -->
  <section class="mb-[26px]">
    <div
      class="flex flex-col items-center gap-1 rounded-[14px] border border-dashed px-6 py-[26px] text-center"
      style="border-color: var(--border);"
    >
      <div class="mb-[10px] flex gap-2">
        {#each PROVIDER_ORDER as p (p)}
          <ProviderTile provider={p} size={30} />
        {/each}
      </div>
      <div class="text-[14px] font-semibold text-[var(--fg)]">Pull your real work into this brief</div>
      <div class="mb-3 max-w-[42ch] text-[12px] leading-[1.5] text-[var(--fg3)]">
        Connect Linear, Jira, Asana, or GitHub and WAID shows your open tasks and notifications right here — no
        tab-switching.
      </div>
      {#if onManage}
        <button
          class="inline-flex h-[30px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-3 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06]"
          onclick={onManage}
        >
          <Icon name="add" size={15} /> Connect a tool
        </button>
      {/if}
    </div>
  </section>
{/if}
