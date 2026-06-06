<script lang="ts">
  import type { Brief } from "$lib/types";
  import { projects, githubUrlInBrief } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import {
    pickDirectory,
    bootstrapFromFolder,
    bootstrapFromGithub,
    bootstrapFromAnswers,
    normalizeBootstrapPaste,
  } from "$lib/tauri";
  import { pastePromptTemplate, blankTemplate } from "$lib/bootstrap";
  import Icon from "./Icon.svelte";

  // Mounted under {#if bootstrapOpen} by ProjectDetail, so this is freshly
  // constructed each open. `onDraft(raw)` hands the proposed file to edit mode —
  // the modal NEVER calls projects.save (the human's Save is the commit gate).
  let { brief, onclose, onDraft }: {
    brief: Brief;
    onclose: () => void;
    onDraft: (raw: string) => void;
  } = $props();

  type View = "menu" | "github" | "interview" | "paste";
  let view = $state<View>("menu");
  // Which method is mid-run (shows a spinner + disables), or null.
  let busy = $state<string | null>(null);

  // Whether an LLM provider is configured — drives the gating copy. Folder /
  // GitHub still work without one (deterministic draft), just no AI body.
  let hasProvider = $derived(projects.llmProvider !== null);
  // A GitHub link already on the brief → recommend GitHub first + prefill it.
  // svelte-ignore state_referenced_locally -- read once at open; the brief is stable while the modal is mounted.
  const existingGithub = githubUrlInBrief(brief);

  // --- method metadata -------------------------------------------------------
  type Method = { id: View | "folder" | "blank"; icon: string; label: string; blurb: string };
  let methods = $derived<Method[]>([
    {
      id: "folder",
      icon: "folder_open",
      label: "Point at a folder or repo",
      blurb: hasProvider
        ? "AI-drafted from your README, manifest & file tree"
        : "Reads metadata only — no AI body (turn on AI in Settings)",
    },
    {
      id: "github",
      icon: "code",
      label: "GitHub URL",
      blurb: hasProvider
        ? "AI-drafted from the repo's description & README"
        : "Pulls description, topics & README — no AI body",
    },
    {
      id: "interview",
      icon: "forum",
      label: "Guided interview",
      blurb: "Answer three quick questions — best for non-code projects",
    },
    {
      id: "paste",
      icon: "content_paste",
      label: "Paste a prompt",
      blurb: "Copy a handoff prompt to any AI, paste the result back",
    },
    {
      id: "blank",
      icon: "description",
      label: "Start from a blank template",
      blurb: "A skeleton brief with the standard sections, ready to fill in",
    },
  ]);

  // Order/recommend by what's available: code briefs with a repo get scan-first;
  // everything else leads with the handoff + interview.
  let ordered = $derived.by(() => {
    const order: Method["id"][] = existingGithub
      ? ["github", "folder", "paste", "interview", "blank"]
      : ["paste", "interview", "folder", "github", "blank"];
    return order.map((id) => methods.find((m) => m.id === id)!);
  });

  // --- form state ------------------------------------------------------------
  let githubUrl = $state(existingGithub ?? "");
  let summary = $state("");
  let goal = $state("");
  let notes = $state("");
  let pasted = $state("");
  let copied = $state(false);

  let template = $derived(pastePromptTemplate(brief.name));

  function pick(id: Method["id"]) {
    if (id === "blank") {
      onDraft(blankTemplate(brief));
    } else if (id === "folder") {
      runFolder();
    } else {
      view = id;
    }
  }

  async function runFolder() {
    if (busy) return;
    busy = "folder";
    try {
      const dir = await pickDirectory();
      if (!dir) return; // cancelled — finally still clears busy
      const raw = await bootstrapFromFolder(brief.path, dir);
      onDraft(raw);
    } catch (e) {
      toasts.error(`Could not read folder: ${e}`);
    } finally {
      busy = null;
    }
  }

  async function runGithub() {
    const url = githubUrl.trim();
    if (!url || busy) return;
    busy = "github";
    try {
      const raw = await bootstrapFromGithub(brief.path, url);
      onDraft(raw);
    } catch (e) {
      toasts.error(`Could not read repo: ${e}`);
    } finally {
      busy = null;
    }
  }

  async function runInterview() {
    if (!summary.trim() || busy) return;
    busy = "interview";
    try {
      const raw = await bootstrapFromAnswers(brief.path, {
        summary: summary.trim(),
        goal: goal.trim() || null,
        notes: notes.trim() || null,
      });
      onDraft(raw);
    } catch (e) {
      toasts.error(`Could not compose brief: ${e}`);
    } finally {
      busy = null;
    }
  }

  async function confirmPaste() {
    const raw = pasted.trim();
    if (!raw || busy) return;
    busy = "paste";
    try {
      // Lift the pasted Current State / Open Questions sections into WAID's
      // app-owned marker regions so a later Refresh regenerates them in place.
      const normalized = await normalizeBootstrapPaste(raw);
      onDraft(normalized);
    } catch (e) {
      toasts.error(`Could not import paste: ${e}`);
    } finally {
      busy = null;
    }
  }

  async function copyTemplate() {
    try {
      await navigator.clipboard.writeText(template);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      toasts.error(`Could not copy: ${e}`);
    }
  }

  let headerTitle = $derived(
    view === "menu"
      ? "Generate the initial brief"
      : view === "github"
        ? "From a GitHub repo"
        : view === "interview"
          ? "Guided interview"
          : "Paste a prompt",
  );

  function goBack() {
    view = "menu";
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<div
  class="anim-fade fixed inset-0 z-50 flex items-start justify-center"
  style="background: color-mix(in srgb, var(--fg) 22%, transparent); backdrop-filter: blur(2px); padding-top: 8vh;"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onclose();
  }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="anim-pop flex max-h-[84vh] w-[min(94%,600px)] flex-col overflow-hidden rounded-[16px] border"
    style="background: var(--bg); border-color: var(--border); box-shadow: var(--shadow-pop);"
    onkeydown={onKeydown}
  >
    <!-- Header -->
    <div class="flex items-center gap-[11px] border-b px-[18px] py-[15px]" style="border-color: var(--border);">
      {#if view !== "menu"}
        <button
          class="-ml-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
          aria-label="Back"
          onclick={goBack}
        >
          <Icon name="arrow_back" size={17} />
        </button>
      {:else}
        <span
          class="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-[9px] text-white"
          style="background: var(--accent);"
        >
          <Icon name="auto_awesome" size={16} />
        </span>
      {/if}

      <div class="min-w-0 flex-1">
        <div class="truncate text-[14.5px] font-semibold text-[var(--fg)]">{headerTitle}</div>
        <div class="mt-px truncate text-[11.5px] text-[var(--fg3)]">{brief.name}</div>
      </div>

      <button
        class="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
        aria-label="Close"
        onclick={onclose}
      >
        <Icon name="close" size={16} />
      </button>
    </div>

    <!-- Body -->
    <div class="scroll-thin flex-1 overflow-y-auto p-[18px]">
      {#if view === "menu"}
        <p class="mb-3 text-[12.5px] leading-[1.5] text-[var(--fg3)]">
          Choose how to fill <strong class="text-[var(--fg2)]">{brief.name}</strong>'s brief. Whatever you pick lands in
          edit mode for you to review — nothing is saved until you hit Save.
        </p>
        <div class="flex flex-col gap-[9px]">
          {#each ordered as m (m.id)}
            <button
              class="group flex items-center gap-[12px] rounded-[12px] border bg-[var(--bg)] p-[13px] text-left transition-all hover:-translate-y-px hover:shadow-[0_4px_14px_rgba(15,30,60,0.08)] disabled:opacity-50"
              style="border-color: var(--border);"
              disabled={busy !== null}
              onclick={() => pick(m.id)}
            >
              <span
                class="grid h-[38px] w-[38px] shrink-0 place-items-center rounded-[11px] text-[var(--fg2)]"
                style="background: var(--chip-bg);"
              >
                <Icon name={busy === m.id ? "progress_activity" : m.icon} size={19} class={busy === m.id ? "spin" : ""} />
              </span>
              <div class="min-w-0 flex-1">
                <div class="text-[13px] font-semibold text-[var(--fg)]">{m.label}</div>
                <div class="text-[11px] leading-[1.4] text-[var(--fg3)]">{m.blurb}</div>
              </div>
              <Icon name="chevron_right" size={18} class="text-[var(--fg3)]" />
            </button>
          {/each}
        </div>

      {:else if view === "github"}
        <label class="flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">GitHub repository URL</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="h-[36px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder="https://github.com/owner/repo"
            autofocus
            bind:value={githubUrl}
            onkeydown={(e) => {
              if (e.key === "Enter") runGithub();
            }}
          />
        </label>
        <p class="mt-2 text-[11px] leading-[1.5] text-[var(--fg3)]">
          Pulls the repo's description, topics & README.
          {hasProvider
            ? " The README is drafted into a brief by your AI provider."
            : " Add an AI provider in Settings to also draft body prose."}
          For private repos, save a GitHub token in Settings.
        </p>

      {:else if view === "interview"}
        <label class="mb-3 flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">What is this project? <span class="text-[var(--status-blocked)]">*</span></span>
          <textarea
            class="min-h-[64px] resize-y rounded-[9px] border p-[10px] text-[12.5px] leading-[1.5] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder="A local-first dashboard for the state of all my projects…"
            bind:value={summary}
          ></textarea>
        </label>
        <label class="mb-3 flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">What's the goal / definition of done?</span>
          <textarea
            class="min-h-[52px] resize-y rounded-[9px] border p-[10px] text-[12.5px] leading-[1.5] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder="Ship a v1 that lists every brief and lets me launch into work."
            bind:value={goal}
          ></textarea>
        </label>
        <label class="flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">Anything else / current state?</span>
          <textarea
            class="min-h-[52px] resize-y rounded-[9px] border p-[10px] text-[12.5px] leading-[1.5] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder="Early prototype. Backend works; UI is rough."
            bind:value={notes}
          ></textarea>
        </label>
        <p class="mt-2 text-[11px] leading-[1.5] text-[var(--fg3)]">
          {hasProvider
            ? "Your answers are drafted into a brief by your AI provider."
            : "Composed into a brief locally — no AI provider needed."}
        </p>

      {:else if view === "paste"}
        <p class="mb-2 text-[12px] leading-[1.5] text-[var(--fg3)]">
          Copy this prompt into any AI that can see your project (e.g. a Claude Project with the docs), then paste its
          output below.
        </p>
        <div class="relative mb-3">
          <pre
            class="scroll-thin max-h-[180px] overflow-y-auto rounded-[10px] border p-3 font-mono text-[11px] leading-[1.55] text-[var(--fg-body)]"
            style="background: var(--code-bg); border-color: var(--border); white-space: pre-wrap;">{template}</pre>
          <button
            class="absolute right-2 top-2 inline-flex h-[26px] items-center gap-[5px] rounded-[7px] border bg-[var(--bg)] px-[9px] text-[11px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
            style="border-color: var(--border);"
            onclick={copyTemplate}
          >
            <Icon name={copied ? "check" : "content_copy"} size={13} />
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
        <label class="flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">Paste the result</span>
          <textarea
            class="min-h-[120px] resize-y rounded-[9px] border p-[10px] font-mono text-[11.5px] leading-[1.6] text-[var(--fg-body)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder="---&#10;name: …&#10;---&#10;&#10;# …"
            bind:value={pasted}
          ></textarea>
        </label>
      {/if}
    </div>

    <!-- Footer (action views only) -->
    {#if view === "github"}
      <div class="flex items-center justify-end border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={runGithub}
          disabled={busy !== null || !githubUrl.trim()}
        >
          {#if busy === "github"}
            <Icon name="progress_activity" size={14} class="spin" /> Reading…
          {:else}
            <Icon name="auto_awesome" size={14} /> Generate draft
          {/if}
        </button>
      </div>
    {:else if view === "interview"}
      <div class="flex items-center justify-end border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={runInterview}
          disabled={busy !== null || !summary.trim()}
        >
          {#if busy === "interview"}
            <Icon name="progress_activity" size={14} class="spin" /> Composing…
          {:else}
            <Icon name="auto_awesome" size={14} /> Generate draft
          {/if}
        </button>
      </div>
    {:else if view === "paste"}
      <div class="flex items-center justify-end border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={confirmPaste}
          disabled={busy !== null || !pasted.trim()}
        >
          {#if busy === "paste"}
            <Icon name="progress_activity" size={14} class="spin" /> Importing…
          {:else}
            <Icon name="check" size={14} /> Use this draft
          {/if}
        </button>
      </div>
    {/if}
  </div>
</div>
