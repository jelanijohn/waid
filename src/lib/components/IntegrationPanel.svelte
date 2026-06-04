<script lang="ts">
  import { onMount } from "svelte";
  import type { Brief, BriefIntegration, IntegrationFetch, IntegrationItem } from "$lib/types";
  import { integrations } from "$lib/stores/integrations.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { openExternal, appendCapture } from "$lib/tauri";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { relativeTime } from "$lib/time";
  import MarkdownView from "./MarkdownView.svelte";
  import Icon from "./Icon.svelte";

  let { brief, onManage }: { brief: Brief; onManage?: () => void } = $props();

  // The AI digest layers over the local rollup; only offered when a synthesis
  // provider (Ollama / Anthropic) is configured.
  let canDigest = $derived(projects.llmProvider !== null);
  let digest = $derived(integrations.digestFor(brief.path));

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

  // Map a kind to a Material glyph for the section header.
  function kindIcon(kind: string): string {
    return kind === "notifications" ? "notifications" : "checklist";
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
  <section class="mb-[26px] flex flex-col gap-4">
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

    {#each brief.integrations as ig (ig.connection + ig.kind)}
      {@const conn = connFor(ig.connection)}
      {@const entry = integrations.get(brief.path, ig.connection, ig.kind)}
      <div class="rounded-[11px] border" style="border-color: var(--border);">
        <!-- Header -->
        <div
          class="flex items-center gap-2 border-b px-[13px] py-[9px]"
          style="border-color: var(--border);"
        >
          <Icon name={kindIcon(ig.kind)} size={15} class="text-[var(--fg2)]" />
          <span class="text-[12.5px] font-semibold text-[var(--fg)]">
            {conn?.label ?? ig.connection}
          </span>
          <span
            class="rounded-md bg-[var(--chip-bg)] px-[6px] py-px text-[10px] font-medium uppercase tracking-[0.04em] text-[var(--fg3)]"
          >{ig.kind}</span>
          {#if entry?.data}
            <span class="truncate text-[11.5px] text-[var(--fg3)]">· {summaryLine(entry.data)}</span>
          {/if}
          <button
            class="ml-auto grid h-[26px] w-[26px] place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
            title="Refresh"
            aria-label="Refresh {conn?.label ?? ig.connection}"
            onclick={() => refresh(ig)}
            disabled={entry?.loading}
          >
            <Icon name="sync" size={14} class={entry?.loading ? "spin" : ""} />
          </button>
        </div>

        <!-- Body: connection missing / error / loading / empty / items -->
        {#if !conn}
          <div class="px-[13px] py-3 text-[12px] text-[var(--fg3)]">
            Connection <code class="text-[var(--fg2)]">{ig.connection}</code> isn't set up on this brief.
            {#if onManage}
              <button class="text-[var(--accent)] hover:underline" onclick={onManage}>Manage integrations</button>.
            {/if}
          </div>
        {:else if entry?.error && !entry.data}
          <div class="px-[13px] py-3 text-[12px] text-[var(--status-blocked)]">
            {entry.error}
          </div>
        {:else if entry?.loading && !entry.data}
          <div class="flex items-center gap-2 px-[13px] py-3 text-[12px] text-[var(--fg3)]">
            <Icon name="sync" size={14} class="spin" /> Loading…
          </div>
        {:else if entry?.data && entry.data.items.length === 0}
          <div class="px-[13px] py-3 text-[12px] text-[var(--fg3)]">Nothing assigned right now. 🎉</div>
        {:else if entry?.data}
          <ul>
            {#each entry.data.items as item (item.id)}
              <li class="border-b last:border-b-0" style="border-color: var(--border);">
                <button
                  class="flex w-full items-start gap-[10px] px-[13px] py-[9px] text-left transition-colors hover:bg-[var(--hover)]"
                  onclick={() => open(item)}
                >
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="truncate text-[12.5px] font-medium text-[var(--fg)]">{item.title}</span>
                      {#if item.status}
                        <span
                          class="shrink-0 rounded-md bg-[var(--chip-bg)] px-[6px] py-px text-[10px] font-medium text-[var(--fg2)]"
                        >{item.status}</span>
                      {/if}
                    </div>
                    <div class="mt-[3px] flex flex-wrap items-center gap-x-[10px] gap-y-1 text-[11px] text-[var(--fg3)]">
                      {#if item.meta?.priority}
                        <span class="inline-flex items-center gap-[3px]">
                          <Icon name="flag" size={11} /> {item.meta.priority}
                        </span>
                      {/if}
                      {#if item.meta?.project}
                        <span class="inline-flex items-center gap-[3px]">
                          <Icon name="folder" size={11} /> {item.meta.project}
                        </span>
                      {/if}
                      {#if item.assignee}
                        <span class="inline-flex items-center gap-[3px]">
                          <Icon name="person" size={11} /> {item.assignee}
                        </span>
                      {/if}
                      {#if item.updatedAt}
                        <span class="inline-flex items-center gap-[3px]">
                          <Icon name="schedule" size={11} /> {relativeTime(item.updatedAt)}
                        </span>
                      {/if}
                    </div>
                  </div>
                  <Icon name="north_east" size={13} class="mt-[2px] shrink-0 text-[var(--fg3)]" />
                </button>
              </li>
            {/each}
          </ul>
          {#if entry.error}
            <!-- Stale data shown above, but the latest refresh failed. -->
            <div class="px-[13px] py-2 text-[11px] text-[var(--status-blocked)]">
              Couldn't refresh: {entry.error}
            </div>
          {/if}
        {/if}
      </div>
    {/each}
  </section>
{/if}
