<script lang="ts">
  import type { Brief, BriefIntegration, Connection, Provider } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import {
    saveBriefConnection,
    deleteBriefConnection,
    saveBriefIntegration,
    deleteBriefIntegration,
    testBriefConnection,
  } from "$lib/tauri";
  import Icon from "./Icon.svelte";

  let { brief, open, onclose }: { brief: Brief; open: boolean; onclose: () => void } = $props();

  const PROVIDERS: { value: Provider; label: string }[] = [
    { value: "linear", label: "Linear" },
    { value: "jira", label: "Jira" },
    { value: "asana", label: "Asana" },
    { value: "github", label: "GitHub" },
  ];

  // --- New-connection form ---------------------------------------------------
  let provider = $state<Provider>("linear");
  let label = $state("");
  let id = $state("");
  let token = $state("");
  let baseUrl = $state("");
  let account = $state("");
  let busy = $state(false);
  let testing = $state<string | null>(null);

  function slugify(s: string): string {
    return s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  }

  let effectiveId = $derived(id.trim() || slugify(label));
  let needsBaseUrl = $derived(provider === "jira" || provider === "github");
  let needsAccount = $derived(provider === "jira" || provider === "asana");
  let accountLabel = $derived(provider === "asana" ? "Workspace ID (optional)" : "Account email");
  let accountPlaceholder = $derived(provider === "asana" ? "1200000000000000" : "you@acme.com");
  let jiraReady = $derived(
    provider !== "jira" || (baseUrl.trim().length > 0 && account.trim().length > 0),
  );
  // Block adding a duplicate id (would silently overwrite the existing one).
  let idTaken = $derived(brief.connections.some((c) => c.id === effectiveId));
  let canSaveConn = $derived(
    !busy &&
      label.trim().length > 0 &&
      effectiveId.length > 0 &&
      token.trim().length > 0 &&
      jiraReady &&
      !idTaken,
  );

  function resetConnForm() {
    provider = "linear";
    label = "";
    id = "";
    token = "";
    baseUrl = "";
    account = "";
  }

  async function saveConn() {
    if (!canSaveConn) return;
    busy = true;
    try {
      const conn: Connection = {
        id: effectiveId,
        provider,
        label: label.trim(),
        baseUrl: needsBaseUrl && baseUrl.trim() ? baseUrl.trim() : null,
        account: needsAccount && account.trim() ? account.trim() : null,
      };
      const updated = await saveBriefConnection(brief.path, conn, token.trim());
      projects.upsert(updated);
      toasts.success(`Connection "${conn.label}" added`);
      resetConnForm();
    } catch (e) {
      toasts.error(`Could not save connection: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function testConn(c: Connection) {
    testing = c.id;
    try {
      await testBriefConnection(brief.path, c.id);
      toasts.success(`${c.label} connected ✓`);
    } catch (e) {
      toasts.error(`${c.label}: ${e}`);
    } finally {
      testing = null;
    }
  }

  async function removeConn(c: Connection) {
    try {
      const updated = await deleteBriefConnection(brief.path, c.id);
      projects.upsert(updated);
      toasts.push(`Removed "${c.label}"`, "info");
    } catch (e) {
      toasts.error(`Could not remove: ${e}`);
    }
  }

  // --- New-integration form --------------------------------------------------
  let igConnection = $state("");
  let igKind = $state<"tasks" | "notifications">("tasks");
  let igQuery = $state("");
  let igLimit = $state("");

  // Default the connection picker to the brief's first connection.
  $effect(() => {
    if (!igConnection && brief.connections.length) igConnection = brief.connections[0].id;
  });

  let selectedConn = $derived(brief.connections.find((c) => c.id === igConnection) ?? null);
  // Only GitHub exposes notifications today.
  let availableKinds = $derived(
    selectedConn?.provider === "github"
      ? (["tasks", "notifications"] as const)
      : (["tasks"] as const),
  );
  // Reset the kind when switching to a connection that doesn't support it
  // (e.g. github→linear while "notifications" was selected).
  $effect(() => {
    if (!(availableKinds as readonly string[]).includes(igKind)) igKind = "tasks";
  });
  let canAddIg = $derived(!busy && igConnection.length > 0);

  function connLabel(connId: string): string {
    return brief.connections.find((c) => c.id === connId)?.label ?? connId;
  }

  async function addIg() {
    if (!canAddIg) return;
    busy = true;
    try {
      const kind = (availableKinds as readonly string[]).includes(igKind) ? igKind : "tasks";
      const integration: BriefIntegration = {
        connection: igConnection,
        kind,
        query: igQuery.trim() || null,
        limit: igLimit.trim() ? Number(igLimit) : null,
      };
      const updated = await saveBriefIntegration(brief.path, integration);
      projects.upsert(updated);
      toasts.success("Integration added");
      igQuery = "";
      igLimit = "";
    } catch (e) {
      toasts.error(`Could not add integration: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function removeIg(ig: BriefIntegration) {
    try {
      const updated = await deleteBriefIntegration(brief.path, ig.connection, ig.kind);
      projects.upsert(updated);
      toasts.push("Integration removed", "info");
    } catch (e) {
      toasts.error(`Could not remove: ${e}`);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

{#if open}
  <div
    class="anim-fade fixed inset-0 z-50 flex items-start justify-center"
    style="background: rgba(10,20,40,0.45); padding-top: 10vh;"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="anim-pop flex max-h-[80vh] w-[min(92%,580px)] flex-col rounded-[14px] border shadow-[0_24px_60px_rgba(10,20,40,0.4)]"
      style="background: var(--bg); border-color: var(--border);"
      onkeydown={onKeydown}
    >
      <div class="flex items-center justify-between gap-3 border-b px-4 py-3" style="border-color: var(--border);">
        <h3 class="flex min-w-0 items-center gap-[7px] text-[14px] font-semibold text-[var(--fg)]">
          <Icon name="hub" size={16} />
          <span class="truncate">Integrations · {brief.name}</span>
        </h3>
        <button
          class="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
          aria-label="Close"
          onclick={onclose}
        >
          <Icon name="close" size={16} />
        </button>
      </div>

      <div class="scroll-thin flex-1 overflow-y-auto p-4">
        <!-- Integrations (selectors) -->
        <div class="mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
          Integrations on this brief
        </div>
        {#if brief.integrations.length}
          <div class="mb-3 flex flex-col gap-1.5">
            {#each brief.integrations as ig (ig.connection + ig.kind)}
              <div
                class="flex items-center gap-2 rounded-[9px] border px-3 py-1.5"
                style="border-color: var(--border);"
              >
                <Icon name={ig.kind === "notifications" ? "notifications" : "checklist"} size={13} class="text-[var(--fg3)]" />
                <span class="truncate text-[12px] font-medium text-[var(--fg)]">{connLabel(ig.connection)}</span>
                <span class="rounded bg-[var(--chip-bg)] px-[5px] py-px text-[9.5px] font-medium uppercase text-[var(--fg3)]">{ig.kind}</span>
                {#if ig.query}
                  <span class="truncate font-mono text-[10.5px] text-[var(--fg3)]">{ig.query}</span>
                {/if}
                <button
                  class="ml-auto grid h-[24px] w-[24px] shrink-0 place-items-center rounded-md text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)]"
                  title="Remove integration"
                  aria-label="Remove integration"
                  onclick={() => removeIg(ig)}
                >
                  <Icon name="close" size={14} />
                </button>
              </div>
            {/each}
          </div>
        {:else}
          <p class="mb-3 text-[11.5px] text-[var(--fg3)]">No integrations yet.</p>
        {/if}

        <!-- Add integration -->
        {#if brief.connections.length}
          <div class="mb-4 flex flex-wrap items-end gap-2 rounded-[10px] border p-2.5" style="border-color: var(--border);">
            <label class="flex min-w-[120px] flex-1 flex-col gap-1 text-[10.5px] text-[var(--fg3)]">
              Connection
              <select
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                bind:value={igConnection}
              >
                {#each brief.connections as c (c.id)}
                  <option value={c.id}>{c.label}</option>
                {/each}
              </select>
            </label>
            {#if availableKinds.length > 1}
              <!-- Only GitHub exposes a second kind (notifications); for the
                   other providers there's just "tasks", so the dropdown is
                   noise and we hide it. -->
              <label class="flex flex-col gap-1 text-[10.5px] text-[var(--fg3)]">
                Kind
                <select
                  class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                  style="background: var(--input-bg); border-color: var(--border);"
                  bind:value={igKind}
                >
                  {#each availableKinds as k (k)}
                    <option value={k}>{k}</option>
                  {/each}
                </select>
              </label>
            {/if}
            <label class="flex w-[64px] flex-col gap-1 text-[10.5px] text-[var(--fg3)]">
              Limit
              <input
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="number"
                min="1"
                placeholder="20"
                bind:value={igLimit}
              />
            </label>
            <label class="flex min-w-[140px] flex-[2] flex-col gap-1 text-[10.5px] text-[var(--fg3)]">
              Query (optional)
              <input
                class="rounded-md border px-2 py-1 font-mono text-[11.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                placeholder={selectedConn?.provider === "jira" ? "project = X AND …" : "provider-specific"}
                bind:value={igQuery}
              />
            </label>
            <button
              class="inline-flex h-[30px] shrink-0 items-center gap-[5px] rounded-lg bg-[var(--accent)] px-3 text-[12px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
              onclick={addIg}
              disabled={!canAddIg}
            >
              <Icon name="add" size={14} /> Add
            </button>
          </div>
        {:else}
          <p class="mb-4 text-[11.5px] text-[var(--fg3)]">Add a connection below first.</p>
        {/if}

        <!-- Connections -->
        <div
          class="mb-1 border-t pt-3 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]"
          style="border-color: var(--border);"
        >
          Connections (this brief)
        </div>
        {#if brief.connections.length}
          <div class="mb-3 flex flex-col gap-2">
            {#each brief.connections as c (c.id)}
              <div class="flex items-center gap-2 rounded-[10px] border px-3 py-2" style="border-color: var(--border);">
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <span class="truncate text-[12.5px] font-medium text-[var(--fg)]">{c.label}</span>
                    <span class="rounded-md bg-[var(--chip-bg)] px-[6px] py-px text-[10px] font-medium text-[var(--fg3)]">
                      {PROVIDERS.find((p) => p.value === c.provider)?.label ?? c.provider}
                    </span>
                  </div>
                  <div class="truncate font-mono text-[10.5px] text-[var(--fg3)]">{c.id}</div>
                </div>
                <button
                  class="inline-flex h-[28px] items-center gap-[5px] rounded-lg border bg-[var(--bg)] px-2.5 text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
                  style="border-color: var(--border);"
                  onclick={() => testConn(c)}
                  disabled={testing === c.id}
                >
                  <Icon name="wifi_tethering" size={13} class={testing === c.id ? "spin" : ""} />
                  {testing === c.id ? "Testing…" : "Test"}
                </button>
                <button
                  class="grid h-[28px] w-[28px] place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)]"
                  title="Remove connection"
                  aria-label="Remove {c.label}"
                  onclick={() => removeConn(c)}
                >
                  <Icon name="delete" size={15} />
                </button>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Add connection -->
        <div class="rounded-[11px] border p-3" style="border-color: var(--border);">
          <div class="mb-2 text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
            Add a connection
          </div>
          <div class="grid grid-cols-2 gap-2">
            <label class="flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
              Provider
              <select
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                bind:value={provider}
              >
                {#each PROVIDERS as p (p.value)}
                  <option value={p.value}>{p.label}</option>
                {/each}
              </select>
            </label>
            <label class="flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
              Label
              <input
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                placeholder="Linear (work)"
                bind:value={label}
              />
            </label>
          </div>

          <label class="mt-2 flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
            Connection id
            <input
              class="rounded-md border px-2 py-1 font-mono text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="text"
              placeholder={slugify(label) || "linear-work"}
              bind:value={id}
            />
            <span class="text-[10.5px] {idTaken ? 'text-[var(--status-blocked)]' : 'text-[var(--fg3)]'}">
              {#if idTaken}
                <code>{effectiveId}</code> is already used on this brief.
              {:else}
                Defaults to <code>{effectiveId || "…"}</code>.
              {/if}
            </span>
          </label>

          {#if needsBaseUrl}
            <label class="mt-2 flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
              Base URL {provider === "github" ? "(Enterprise — optional)" : ""}
              <input
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                placeholder={provider === "jira" ? "https://acme.atlassian.net" : "https://github.example.com"}
                bind:value={baseUrl}
              />
            </label>
          {/if}
          {#if needsAccount}
            <label class="mt-2 flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
              {accountLabel}
              <input
                class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="text"
                placeholder={accountPlaceholder}
                bind:value={account}
              />
            </label>
          {/if}

          <label class="mt-2 flex flex-col gap-1 text-[11px] text-[var(--fg3)]">
            API token
            <input
              class="rounded-md border px-2 py-1 text-[12px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="password"
              autocomplete="off"
              placeholder="Stored in your OS keychain"
              bind:value={token}
            />
          </label>

          <div class="mt-3 flex items-center justify-between gap-2">
            <span class="text-[10.5px] text-[var(--fg3)]">
              The token goes straight to your OS keychain — never the brief.
            </span>
            <button
              class="inline-flex h-[30px] shrink-0 items-center rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
              onclick={saveConn}
              disabled={!canSaveConn}
            >
              {busy ? "Saving…" : "Add connection"}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
