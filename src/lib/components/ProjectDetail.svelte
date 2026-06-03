<script lang="ts">
  import type { Brief, Webhook } from "$lib/types";
  import { projects, isSyncableBrief } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { openExternal, fireWebhook } from "$lib/tauri";
  import { relativeTime } from "$lib/time";
  import { statusColor } from "$lib/status";
  import MarkdownView from "./MarkdownView.svelte";
  import Icon from "./Icon.svelte";

  let { brief }: { brief: Brief } = $props();

  // This component is mounted under {#key brief.path}, so local state resets
  // automatically when the selection changes — no effects needed.
  let editing = $state(false);
  // svelte-ignore state_referenced_locally -- intentional: {#key brief.path} remounts this component, re-seeding draft from the new brief.
  let draft = $state(brief.raw);
  let saving = $state(false);
  let syncing = $state(false);

  // Whether this brief has anything to sync (a GitHub link or explicit source).
  let syncable = $derived(isSyncableBrief(brief));
  // Whether an LLM synthesis provider is configured (enables Current State +
  // Open Questions on Refresh, even for briefs whose only signal is Captures).
  let canSynthesize = $derived(projects.llmProvider !== null);
  // Refresh is offered when there's deterministic data to sync OR a provider to
  // synthesize with.
  let canRefresh = $derived(syncable || canSynthesize);

  // Obsidian deep link (null unless the briefs dir is inside a vault).
  let obsidianUri = $derived(projects.obsidianUri(brief));
  // Briefs that wikilink to this one.
  let backlinks = $derived(projects.backlinksFor(brief.path));

  // WSL can't reach the Windows host without a URL handler; hint at the fix.
  let wslHint = $derived(
    projects.isWsl
      ? " WSL needs a URL handler — install wslu so wslview forwards links to Windows."
      : "",
  );

  // Pick a Material glyph for a launch link based on its label.
  function iconForLink(label: string): string {
    const l = label.toLowerCase();
    if (l.includes("github")) return "code";
    if (l.includes("docs") || l.includes("tauri")) return "menu_book";
    if (l.includes("obsidian")) return "hub";
    if (l.includes("claude")) return "auto_awesome";
    return "north_east";
  }

  async function openInObsidian() {
    if (!obsidianUri) return;
    try {
      await openExternal(obsidianUri);
    } catch (e) {
      toasts.error(
        `Could not open Obsidian (${e}). Make sure Obsidian is installed and this vault is open in it.${wslHint}`,
      );
    }
  }

  async function save() {
    saving = true;
    try {
      await projects.save(brief.path, draft);
      editing = false;
      toasts.success("Saved");
    } catch (e) {
      toasts.error(`Save failed: ${e}`);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    draft = brief.raw;
    editing = false;
  }

  async function openLink(url: string) {
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(`Could not open link (${e}).${wslHint}`);
    }
  }

  async function fire(hook: Webhook) {
    try {
      const res = await fireWebhook(hook.url, hook.method, hook.body);
      if (res.ok) {
        toasts.success(`${hook.label || "Webhook"} → ${res.status}`);
      } else {
        toasts.error(`${hook.label || "Webhook"} → ${res.status}`);
      }
    } catch (e) {
      toasts.error(`${hook.label || "Webhook"} failed: ${e}`);
    }
  }

  async function refresh() {
    if (syncing) return;
    syncing = true;
    try {
      // Deterministic Activity sync first, then (when configured) LLM synthesis
      // of Current State + Open Questions. Each step is independent so a sync
      // failure still reports clearly.
      if (syncable) await projects.sync(brief.path);
      if (canSynthesize) await projects.synthesize(brief.path);
      toasts.success(canSynthesize ? "Refreshed" : "Synced");
    } catch (e) {
      toasts.error(`Refresh failed: ${e}`);
    } finally {
      syncing = false;
    }
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "s") {
      e.preventDefault();
      save();
    }
  }
</script>

<div class="flex h-screen flex-col bg-[var(--bg)] text-[var(--fg)]">
  <!-- Header -->
  <header
    class="relative overflow-hidden border-b"
    style="border-color: var(--border); padding: var(--pad-y) var(--pad-x) 16px;"
  >
    <!-- Faint diagonal accent wash -->
    <div
      class="pointer-events-none absolute inset-0"
      style="background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 9%, transparent), transparent 55%);"
    ></div>

    <div class="relative">
      <!-- Title row -->
      <div class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <div class="flex items-center gap-[9px]">
            {#if brief.status}
              <span class="sdot h-2 w-2" style="--sc: {statusColor(brief.status)};"></span>
            {/if}
            <h2 class="truncate font-bold tracking-[-0.02em] text-[var(--fg)]" style="font-size: var(--title-size);">
              {brief.name}
            </h2>
          </div>
          {#if brief.description}
            <p class="mt-[5px] max-w-[62ch] text-[13.5px] leading-[1.5] text-[var(--fg2)]">
              {brief.description}
            </p>
          {/if}
        </div>

        <div class="flex shrink-0 gap-2">
          {#if editing}
            <button
              class="inline-flex h-[30px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-3 text-[12.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
              style="border-color: var(--border);"
              onclick={cancel}
              disabled={saving}
            >
              Cancel
            </button>
            <button
              class="inline-flex h-[30px] items-center gap-[5px] rounded-lg border border-transparent bg-[var(--accent)] px-3 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
              onclick={save}
              disabled={saving}
            >
              {saving ? "Saving…" : "Save"}
            </button>
          {:else}
            {#if canRefresh}
              <button
                class="inline-flex h-[30px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-3 text-[12.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
                style="border-color: var(--border);"
                title={canSynthesize
                  ? "Refresh activity and re-synthesize Current State + Open Questions"
                  : "Refresh synced data from linked integrations"}
                onclick={refresh}
                disabled={syncing}
              >
                <Icon name="sync" size={14} class={syncing ? "spin" : ""} />
                {syncing ? "Refreshing…" : "Refresh"}
              </button>
            {/if}
            {#if obsidianUri}
              <button
                class="inline-flex h-[30px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-3 text-[12.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
                style="border-color: var(--border);"
                title="Open this brief in Obsidian"
                onclick={openInObsidian}
              >
                <Icon name="hub" size={14} /> Obsidian
              </button>
            {/if}
            <button
              class="inline-flex h-[30px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-3 text-[12.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
              style="border-color: var(--border);"
              onclick={() => {
                draft = brief.raw;
                editing = true;
              }}
            >
              <Icon name="edit" size={14} /> Edit
            </button>
          {/if}
        </div>
      </div>

      <!-- Tags + opened -->
      <div class="mt-[14px] flex flex-wrap items-center gap-[7px]">
        {#each brief.tags as tag (tag)}
          <span class="rounded-md bg-[var(--chip-bg)] px-2 py-[2px] text-[11.5px] text-[var(--fg2)]">#{tag}</span>
        {/each}
        <span class="ml-[2px] inline-flex items-center gap-1 text-[11.5px] text-[var(--fg3)]">
          <Icon name="schedule" size={13} /> Opened {relativeTime(brief.lastOpened)}
        </span>
        {#if brief.lastSynced}
          <span class="inline-flex items-center gap-1 text-[11.5px] text-[var(--fg3)]">
            <Icon name="sync" size={13} /> Synced {relativeTime(brief.lastSynced)}
          </span>
        {/if}
      </div>

      <!-- Launch row: links + webhooks -->
      {#if brief.links.length || brief.webhooks.length}
        <div class="mt-4 flex flex-wrap gap-2">
          {#each brief.links as link (link.url + link.label)}
            <button
              class="inline-flex h-[30px] items-center gap-[6px] rounded-lg border px-[11px] text-[12px] font-medium transition-[filter] hover:brightness-[1.08]"
              style="background: var(--launch-bg); color: var(--launch-fg); border-color: var(--launch-bd);"
              onclick={() => openLink(link.url)}
            >
              <Icon name={iconForLink(link.label || link.url)} size={15} />
              {link.label || link.url}
              <Icon name="north_east" size={12} class="opacity-50" />
            </button>
          {/each}
          {#each brief.webhooks as hook (hook.url + hook.label)}
            <button
              class="inline-flex h-[30px] items-center gap-[6px] rounded-lg border px-[11px] text-[12px] font-medium transition-[filter] hover:brightness-[1.08]"
              style="background: var(--hook-bg); color: var(--hook-fg); border-color: var(--hook-bd);"
              title={`${hook.method} ${hook.url}`}
              onclick={() => fire(hook)}
            >
              <Icon name="bolt" size={15} fill={1} />
              {hook.label || hook.url}
              <span class="text-[9px] font-bold uppercase opacity-70">{hook.method}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </header>

  <!-- Body: rendered view or editor -->
  <div class="scroll-thin flex-1 overflow-y-auto" style="padding: var(--body-y) var(--pad-x);">
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="h-full min-h-[56vh] w-full resize-none rounded-[10px] border p-4 font-mono text-[12.5px] leading-[1.65] text-[var(--fg-body)] outline-none focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
        style="background: var(--input-bg); border-color: var(--border);"
        bind:value={draft}
        onkeydown={onEditorKeydown}
        spellcheck="false"
        autofocus
      ></textarea>
      <p class="mt-2 text-[11px] text-[var(--fg3)]">
        Editing the raw file (frontmatter + markdown). ⌘/Ctrl+S to save.
      </p>
    {:else}
      <MarkdownView source={brief.body} />

      {#if backlinks.length}
        <section class="mt-[30px] border-t pt-[18px]" style="border-color: var(--border);">
          <h3 class="mb-[10px] text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--fg3)]">
            Linked from
          </h3>
          <div class="flex flex-wrap gap-[7px]">
            {#each backlinks as link (link.path)}
              <button
                class="inline-flex items-center gap-[5px] rounded-[7px] bg-[var(--chip-bg)] px-[10px] py-1 text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
                onclick={() => projects.select(link.path)}
              >
                <Icon name="subdirectory_arrow_right" size={13} /> {link.name}
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {/if}
  </div>

  <footer
    class="flex gap-[14px] border-t font-mono text-[10.5px] text-[var(--fg3)]"
    style="border-color: var(--border); padding: 7px var(--pad-x);"
  >
    <span class="truncate">{brief.path}</span>
    <span class="ml-auto shrink-0">⌘K capture · ⌘S save · ⌘F search</span>
  </footer>
</div>
