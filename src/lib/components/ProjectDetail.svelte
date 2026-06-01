<script lang="ts">
  import type { Brief, Webhook } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { openExternal, fireWebhook } from "$lib/tauri";
  import { relativeTime } from "$lib/time";
  import MarkdownView from "./MarkdownView.svelte";

  let { brief }: { brief: Brief } = $props();

  // This component is mounted under {#key brief.path}, so local state resets
  // automatically when the selection changes — no effects needed.
  let editing = $state(false);
  let draft = $state(brief.raw);
  let saving = $state(false);

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

  function onEditorKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "s") {
      e.preventDefault();
      save();
    }
  }
</script>

<div class="flex h-screen flex-col">
  <!-- Header -->
  <header class="border-b border-slate-200 px-8 py-5 dark:border-slate-800">
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h2 class="truncate text-2xl font-semibold text-slate-900 dark:text-slate-100">
          {brief.name}
        </h2>
        {#if brief.description}
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">
            {brief.description}
          </p>
        {/if}
      </div>
      <div class="flex shrink-0 items-center gap-2">
        {#if editing}
          <button
            class="rounded-md px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
            onclick={cancel}
            disabled={saving}
          >
            Cancel
          </button>
          <button
            class="rounded-md bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-700 disabled:opacity-50"
            onclick={save}
            disabled={saving}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        {:else}
          {#if obsidianUri}
            <button
              class="inline-flex items-center gap-1.5 rounded-md border border-violet-300 px-3 py-1.5 text-sm font-medium text-violet-700 hover:bg-violet-50 dark:border-violet-500/40 dark:text-violet-300 dark:hover:bg-violet-500/10"
              title="Open this brief in Obsidian"
              onclick={openInObsidian}
            >
              Open in Obsidian ↗
            </button>
          {/if}
          <button
            class="rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
            onclick={() => {
              draft = brief.raw;
              editing = true;
            }}
          >
            Edit
          </button>
        {/if}
      </div>
    </div>

    <!-- Tags + meta -->
    <div class="mt-3 flex flex-wrap items-center gap-2">
      {#each brief.tags as tag (tag)}
        <span
          class="rounded-md bg-slate-100 px-2 py-0.5 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300"
        >
          #{tag}
        </span>
      {/each}
      <span class="text-xs text-slate-400 dark:text-slate-500">
        Opened {relativeTime(brief.lastOpened)}
      </span>
    </div>

    <!-- Action buttons: links + webhooks -->
    {#if brief.links.length || brief.webhooks.length}
      <div class="mt-4 flex flex-wrap gap-2">
        {#each brief.links as link (link.url + link.label)}
          <button
            class="inline-flex items-center gap-1 rounded-md bg-slate-900 px-3 py-1.5 text-sm font-medium text-white hover:bg-slate-700 dark:bg-slate-100 dark:text-slate-900 dark:hover:bg-white"
            onclick={() => openLink(link.url)}
          >
            {link.label || link.url} ↗
          </button>
        {/each}
        {#each brief.webhooks as hook (hook.url + hook.label)}
          <button
            class="inline-flex items-center gap-1 rounded-md border border-amber-400 bg-amber-50 px-3 py-1.5 text-sm font-medium text-amber-800 hover:bg-amber-100 dark:border-amber-500/40 dark:bg-amber-500/10 dark:text-amber-300 dark:hover:bg-amber-500/20"
            title={`${hook.method} ${hook.url}`}
            onclick={() => fire(hook)}
          >
            ⚡ {hook.label || hook.url}
            <span class="text-[10px] opacity-70">{hook.method}</span>
          </button>
        {/each}
      </div>
    {/if}
  </header>

  <!-- Body: rendered view or editor -->
  <div class="flex-1 overflow-y-auto px-8 py-6">
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="h-full min-h-[60vh] w-full resize-none rounded-lg border border-slate-300 bg-white p-4 font-mono text-sm leading-relaxed text-slate-800 outline-none focus:ring-2 focus:ring-sky-500 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200"
        bind:value={draft}
        onkeydown={onEditorKeydown}
        spellcheck="false"
        autofocus
      ></textarea>
      <p class="mt-2 text-xs text-slate-400">
        Editing the raw file (frontmatter + markdown). ⌘/Ctrl+S to save.
      </p>
    {:else}
      <MarkdownView source={brief.body} />

      {#if backlinks.length}
        <section class="mt-10 border-t border-slate-200 pt-4 dark:border-slate-800">
          <h3
            class="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-400 dark:text-slate-500"
          >
            Linked from
          </h3>
          <div class="flex flex-wrap gap-2">
            {#each backlinks as link (link.path)}
              <button
                class="inline-flex items-center gap-1 rounded-md bg-slate-100 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
                onclick={() => projects.select(link.path)}
              >
                {link.name}
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {/if}
  </div>

  <footer
    class="border-t border-slate-200 px-8 py-2 text-[11px] text-slate-400 dark:border-slate-800 dark:text-slate-500"
  >
    {brief.path}
  </footer>
</div>
