<script lang="ts">
  import type { Brief, Webhook, WebhookHeader } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { saveBriefWebhook, deleteBriefWebhook } from "$lib/tauri";
  import Icon from "./Icon.svelte";
  import CredentialHelp from "./CredentialHelp.svelte";

  // Mounted under {#if webhooksOpen} by ProjectDetail, so this is freshly
  // constructed each open — the start view derives from the brief once.
  let { brief, onclose }: { brief: Brief; onclose: () => void } = $props();

  const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"] as const;
  const SECRET_TOKEN = "{{secret}}";

  type View = "list" | "edit";
  // svelte-ignore state_referenced_locally -- intentional: mounted under {#if webhooksOpen}, so the start view is seeded once at open time.
  let view = $state<View>(brief.webhooks.length ? "list" : "edit");
  let busy = $state(false);

  function slugify(s: string): string {
    return s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  }

  // A webhook's effective identity: its persisted slug, or one synthesized from
  // the label — matching the backend's ensure_webhook_ids so both sides agree.
  function effIdOf(w: Webhook): string {
    return (w.id || "").trim() || slugify(w.label);
  }

  function refsSecret(headers: WebhookHeader[]): boolean {
    return headers.some((h) => h.value.includes(SECRET_TOKEN));
  }

  // --- edit form -------------------------------------------------------------
  let editingId = $state<string | null>(null); // null = creating a new webhook
  let label = $state("");
  let url = $state("");
  let method = $state<string>("POST");
  let body = $state("");
  let headers = $state<WebhookHeader[]>([]);
  let secret = $state("");

  let usesSecret = $derived(refsSecret(headers));
  let effectiveId = $derived(editingId ?? slugify(label));
  // Block a label whose slug collides with a *different* webhook (would
  // overwrite it). Mirrors the connection idTaken guard.
  let idTaken = $derived(
    effectiveId.length > 0 &&
      brief.webhooks.some((w) => effIdOf(w) === effectiveId && effIdOf(w) !== editingId),
  );
  let canSave = $derived(
    !busy && label.trim().length > 0 && url.trim().length > 0 && effectiveId.length > 0 && !idTaken,
  );

  function startNew() {
    editingId = null;
    label = "";
    url = "";
    method = "POST";
    body = "";
    headers = [];
    secret = "";
    view = "edit";
  }

  function startEdit(w: Webhook) {
    editingId = effIdOf(w);
    label = w.label;
    url = w.url;
    method = (w.method || "POST").toUpperCase();
    body = w.body ?? "";
    // Clone so edits don't mutate the store's brief in place.
    headers = (w.headers ?? []).map((h) => ({ name: h.name, value: h.value }));
    secret = "";
    view = "edit";
  }

  function addHeader() {
    headers = [...headers, { name: "", value: "" }];
  }

  function addBearer() {
    headers = [...headers, { name: "Authorization", value: `Bearer ${SECRET_TOKEN}` }];
  }

  function removeHeader(i: number) {
    headers = headers.filter((_, idx) => idx !== i);
  }

  async function save() {
    if (!canSave) return;
    busy = true;
    try {
      const webhook: Webhook = {
        id: editingId ?? "", // empty → backend synthesizes the slug from the label
        label: label.trim(),
        url: url.trim(),
        method,
        body: body.trim() ? body : null,
        headers: headers
          .filter((h) => h.name.trim().length > 0)
          .map((h) => ({ name: h.name.trim(), value: h.value })),
      };
      const updated = await saveBriefWebhook(brief.path, webhook, secret.trim());
      projects.upsert(updated);
      toasts.success(`${webhook.label} saved ✓`);
      view = "list";
    } catch (e) {
      toasts.error(`Could not save webhook: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function remove(w: Webhook) {
    busy = true;
    try {
      const updated = await deleteBriefWebhook(brief.path, effIdOf(w));
      projects.upsert(updated);
      toasts.push(`Removed ${w.label || "webhook"}`, "info");
    } catch (e) {
      toasts.error(`Could not remove: ${e}`);
    } finally {
      busy = false;
    }
  }

  function goBack() {
    if (view === "edit") view = brief.webhooks.length ? "list" : "edit";
  }
  let canGoBack = $derived(view === "edit" && brief.webhooks.length > 0);

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
      {#if canGoBack}
        <button
          class="-ml-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-lg text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
          aria-label="Back"
          onclick={goBack}
        >
          <Icon name="arrow_back" size={17} />
        </button>
      {/if}

      <span
        class="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-[9px] text-white"
        style="background: var(--accent);"
      >
        <Icon name="bolt" size={16} fill={1} />
      </span>

      <div class="min-w-0 flex-1">
        <div class="truncate text-[14.5px] font-semibold text-[var(--fg)]">
          {view === "list" ? "Webhooks" : editingId ? "Edit webhook" : "New webhook"}
        </div>
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
      {#if view === "list"}
        <!-- ============ LIST ============ -->
        {#each brief.webhooks as w (effIdOf(w))}
          <div
            class="mb-[10px] flex items-center gap-[11px] rounded-[13px] border px-[13px] py-[11px]"
            style="border-color: var(--border);"
          >
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-[7px]">
                <span class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{w.label || "Untitled"}</span>
                <span class="shrink-0 rounded-[5px] bg-[var(--chip-bg)] px-[6px] py-px text-[9.5px] font-bold uppercase text-[var(--fg2)]">
                  {(w.method || "POST").toUpperCase()}
                </span>
                {#if refsSecret(w.headers ?? [])}
                  <Icon name="lock" size={13} class="shrink-0 text-[var(--fg3)]" />
                {/if}
              </div>
              <div class="mt-px truncate font-mono text-[11px] text-[var(--fg3)]">{w.url}</div>
            </div>
            <button
              class="inline-flex h-[26px] shrink-0 items-center gap-[5px] rounded-[7px] border bg-[var(--bg)] px-[9px] text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
              style="border-color: var(--border);"
              onclick={() => startEdit(w)}
            >
              <Icon name="edit" size={13} /> Edit
            </button>
            <button
              class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)] disabled:opacity-50"
              title="Remove webhook"
              aria-label="Remove {w.label}"
              disabled={busy}
              onclick={() => remove(w)}
            >
              <Icon name="delete" size={15} />
            </button>
          </div>
        {/each}

        <button
          class="inline-flex h-[38px] w-full items-center justify-center gap-[6px] rounded-lg bg-[var(--accent)] text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06]"
          onclick={startNew}
        >
          <Icon name="add" size={16} /> New webhook
        </button>

        <p class="mt-[14px] text-[11px] leading-[1.5] text-[var(--fg3)]">
          A webhook fires an HTTP request when you click its button. Add custom headers to authenticate — put
          <code class="rounded bg-[var(--code-bg)] px-1 py-px font-mono text-[10.5px]">{SECRET_TOKEN}</code> in a header value
          and store the token below. The token lives in your OS keychain, never in the brief.
        </p>
      {:else}
        <!-- ============ EDIT ============ -->
        <label class="mb-3 flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">Label</span>
          <input
            class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: {idTaken ? 'var(--status-blocked)' : 'var(--border)'};"
            placeholder="Deploy staging"
            bind:value={label}
          />
          {#if idTaken}
            <span class="text-[10.5px] text-[var(--status-blocked)]">
              Another webhook already uses the id <code>{effectiveId}</code> — pick a different label.
            </span>
          {/if}
        </label>

        <div class="mb-3 grid grid-cols-[1fr_110px] gap-[10px]">
          <label class="flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">URL</span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] font-mono text-[11.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              placeholder="https://api.example.com/deploy"
              bind:value={url}
            />
          </label>
          <label class="flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">Method</span>
            <select
              class="h-[34px] rounded-[9px] border px-[8px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              bind:value={method}
            >
              {#each METHODS as m (m)}
                <option value={m}>{m}</option>
              {/each}
            </select>
          </label>
        </div>

        <!-- Headers -->
        <div class="mb-3 flex flex-col gap-[6px]">
          <div class="flex items-center justify-between">
            <span class="text-[11px] text-[var(--fg3)]">Headers</span>
            <div class="flex items-center gap-[10px]">
              <button class="text-[11px] font-medium text-[var(--accent)] hover:underline" onclick={addBearer}>
                + Bearer token
              </button>
              <button class="text-[11px] font-medium text-[var(--accent)] hover:underline" onclick={addHeader}>
                + Header
              </button>
            </div>
          </div>
          {#each headers as h, i (i)}
            <div class="flex items-center gap-[6px]">
              <input
                class="h-[32px] w-[38%] rounded-[8px] border px-[9px] font-mono text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                placeholder="Header-Name"
                bind:value={h.name}
              />
              <input
                class="h-[32px] flex-1 rounded-[8px] border px-[9px] font-mono text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                placeholder={`Bearer ${SECRET_TOKEN}`}
                bind:value={h.value}
              />
              <button
                class="grid h-[28px] w-[28px] shrink-0 place-items-center rounded-md text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)]"
                title="Remove header"
                aria-label="Remove header"
                onclick={() => removeHeader(i)}
              >
                <Icon name="close" size={15} />
              </button>
            </div>
          {/each}
          {#if !headers.length}
            <span class="text-[10.5px] text-[var(--fg3)]">
              No custom headers. Add a bearer token to authenticate, or leave empty for an unauthenticated trigger.
            </span>
          {/if}
        </div>

        <!-- Secret (only when a header references it) -->
        {#if usesSecret}
          <label class="mb-3 flex flex-col gap-[5px]">
            <span class="flex items-center justify-between text-[11px] text-[var(--fg3)]">
              <span>Secret (for <code>{SECRET_TOKEN}</code>)</span>
              <CredentialHelp topic="webhook-secret" />
            </span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="password"
              autocomplete="off"
              placeholder={editingId ? "Leave blank to keep existing" : "Paste your token"}
              bind:value={secret}
            />
            <span class="text-[10.5px] text-[var(--fg3)]">
              <Icon name="lock" size={12} class="-mt-px mr-0.5" />Stored in your OS keychain — never written to the brief.
            </span>
          </label>
        {/if}

        <!-- Body -->
        <label class="flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">Body (optional)</span>
          <textarea
            class="min-h-[72px] resize-y rounded-[9px] border p-[10px] font-mono text-[11.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder={'{"env":"staging"}'}
            bind:value={body}
          ></textarea>
          <span class="text-[10.5px] text-[var(--fg3)]">Sent as JSON unless you set your own content-type header.</span>
        </label>
      {/if}
    </div>

    <!-- Footer (edit only) -->
    {#if view === "edit"}
      <div class="flex items-center justify-end gap-3 border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={save}
          disabled={!canSave}
        >
          {#if busy}
            Saving…
          {:else}
            <Icon name="check" size={14} /> Save webhook
          {/if}
        </button>
      </div>
    {/if}
  </div>
</div>
