<script lang="ts">
  import type { Brief, BriefIntegration, Connection, Provider } from "$lib/types";
  import { projects } from "$lib/stores/projects.svelte";
  import { toasts } from "$lib/stores/toasts.svelte";
  import { integrations } from "$lib/stores/integrations.svelte";
  import { PROVIDERS, PROVIDER_ORDER, kindIcon, kindLabel } from "$lib/providers";
  import {
    saveBriefConnection,
    deleteBriefConnection,
    saveBriefIntegration,
    deleteBriefIntegration,
    testBriefConnection,
    connectGmail,
    generateGmailQuery,
  } from "$lib/tauri";
  import Icon from "./Icon.svelte";
  import ProviderTile from "./ProviderTile.svelte";

  // Mounted under {#if open} by ProjectDetail, so this is freshly constructed
  // each time the modal opens — the start view derives from the brief once.
  let { brief, onclose }: { brief: Brief; onclose: () => void } = $props();

  // The whole modal is one small view state machine. The words "connection"
  // and "integration" never surface as separate user tasks: it's "connect a
  // tool", then "what to pull in" (a feed). Internally they still map 1:1 to
  // Connection / BriefIntegration.
  type View = "manage" | "pick" | "connect" | "choose";
  // svelte-ignore state_referenced_locally -- intentional: ProjectDetail mounts this under {#if integrationsOpen}, so the start view is seeded once from the brief at open time.
  let view = $state<View>(brief.integrations.length ? "manage" : "pick");
  let busy = $state(false);

  // --- connect (step 1) form -------------------------------------------------
  let provider = $state<Provider>("linear");
  let label = $state("");
  let token = $state("");
  let baseUrl = $state("");
  let account = $state("");
  let id = $state(""); // only surfaced on a slug collision

  // --- choose (step 2) form, targeting one connection ------------------------
  let targetConnId = $state("");
  let chooseReturn = $state<View>("connect"); // where the back chevron returns
  let kind = $state<string>("tasks");
  let query = $state("");
  let limit = $state("20");

  let testing = $state<string | null>(null);

  // --- inline edit (manage view) ---------------------------------------------
  // Rename a connection's label or edit a feed's query in place. One open at a
  // time. A feed's identity includes its query, so editing it = drop the old
  // selector + save a new one (see saveEditFeed).
  let editingConnId = $state<string | null>(null);
  let editConnLabel = $state("");
  let editingFeedKey = $state<string | null>(null);
  let editFeedQuery = $state("");

  const feedKey = (f: BriefIntegration) => `${f.connection}|${f.kind}|${f.query ?? ""}`;

  // Focus a freshly-revealed edit input (no a11y autofocus warning).
  function focusInput(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // Starter Gmail searches for users who don't know Google's operators. Clicking
  // a chip fills the query; they tweak from there. The first doubles as a useful
  // default applied when a Gmail feed is started: recent unread, with the noisy
  // Promotions/Social/Updates categories filtered out.
  const GMAIL_TEMPLATES: { label: string; query: string }[] = [
    { label: "Recent unread", query: "is:unread newer_than:7d -category:promotions -category:social -category:updates" },
    { label: "Needs my reply", query: "to:me is:unread newer_than:14d -category:promotions -category:social" },
    { label: "Important", query: "is:important newer_than:14d" },
    { label: "Starred", query: "is:starred" },
    { label: "Has attachment", query: "has:attachment newer_than:30d" },
    { label: "From a person", query: "from:name@example.com newer_than:30d" },
    { label: "By label", query: "label:my-label newer_than:30d" },
  ];
  const GMAIL_DEFAULT_QUERY = GMAIL_TEMPLATES[0].query;

  // Natural-language → Gmail query via the configured synthesis LLM.
  let aiPrompt = $state("");
  let aiBusy = $state(false);

  async function generateQuery() {
    const p = aiPrompt.trim();
    if (!p || aiBusy) return;
    aiBusy = true;
    try {
      query = await generateGmailQuery(p);
      toasts.success("Filter generated — tweak it if needed");
    } catch (e) {
      toasts.error(`Could not generate: ${e}`);
    } finally {
      aiBusy = false;
    }
  }

  function slugify(s: string): string {
    return s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  }

  let meta = $derived(PROVIDERS[provider]);
  let effectiveId = $derived(id.trim() || slugify(label));
  // Block a duplicate id (it would silently overwrite the existing connection).
  let idTaken = $derived(brief.connections.some((c) => c.id === effectiveId));
  let jiraReady = $derived(
    provider !== "jira" || (baseUrl.trim().length > 0 && account.trim().length > 0),
  );
  let canConnect = $derived(
    !busy &&
      label.trim().length > 0 &&
      token.trim().length > 0 &&
      effectiveId.length > 0 &&
      jiraReady &&
      !idTaken,
  );
  // Gmail connects via OAuth, not a pasted token — its readiness drops the token
  // requirement (the account is discovered by the flow, not typed).
  let canConnectGmail = $derived(
    !busy && label.trim().length > 0 && effectiveId.length > 0 && !idTaken,
  );

  // Already-connected accounts for the picked provider (offer to reuse).
  let existingForProvider = $derived(brief.connections.filter((c) => c.provider === provider));

  // Manage view: each connection with the feeds referencing it.
  let groups = $derived(
    brief.connections.map((c) => ({
      conn: c,
      feeds: brief.integrations.filter((ig) => ig.connection === c.id),
    })),
  );

  // The connection targeted by the choose step.
  let targetConn = $derived(brief.connections.find((c) => c.id === targetConnId) ?? null);
  let availableKinds = $derived(targetConn ? PROVIDERS[targetConn.provider].kinds : (["tasks"] as const));
  // Notion needs a database/page URL in `query` (it's not an optional filter), and
  // a "Project page" feed is a single page — no Max-items cap.
  let isNotion = $derived(targetConn?.provider === "notion");
  let isNotionPage = $derived(isNotion && kind === "page");
  // Gmail's search query is the feed's assignment (like a Notion URL), so it's
  // required, not an optional filter.
  let isGmail = $derived(targetConn?.provider === "gmail");
  let queryRequired = $derived(isNotion || isGmail);
  let canAddFeed = $derived(!busy && (!queryRequired || query.trim().length > 0));

  // --- header copy -----------------------------------------------------------
  let headerTitle = $derived(
    view === "manage"
      ? "Connected tools"
      : view === "pick"
        ? "Connect a tool"
        : view === "connect"
          ? `Connect ${meta.label}`
          : "What should we pull in?",
  );
  let headerSub = $derived(
    view === "manage"
      ? brief.name
      : view === "pick"
        ? "Pick where your tasks live"
        : view === "connect"
          ? "Step 1 of 2 · Sign in"
          : "Step 2 of 2 · Choose a view",
  );

  // --- flow ------------------------------------------------------------------
  function pickProvider(p: Provider) {
    provider = p;
    label = `${PROVIDERS[p].label} — work`;
    token = "";
    baseUrl = "";
    account = "";
    id = "";
    view = "connect";
  }

  function startChoose(connId: string, ret: View) {
    targetConnId = connId;
    const conn = brief.connections.find((c) => c.id === connId);
    kind = conn ? PROVIDERS[conn.provider].kinds[0] : "tasks";
    query = conn?.provider === "gmail" ? GMAIL_DEFAULT_QUERY : "";
    aiPrompt = "";
    limit = "20";
    chooseReturn = ret;
    view = "choose";
  }

  async function connect() {
    if (!canConnect) return;
    busy = true;
    try {
      const conn: Connection = {
        id: effectiveId,
        provider,
        label: label.trim(),
        baseUrl: meta.needsBaseUrl && baseUrl.trim() ? baseUrl.trim() : null,
        account: meta.needsAccount && account.trim() ? account.trim() : null,
      };
      const updated = await saveBriefConnection(brief.path, conn, token.trim());
      projects.upsert(updated);
      toasts.success(`${conn.label} connected ✓`);
      // The whole fix: completing auth carries you into step 2 (do NOT close).
      startChoose(conn.id, "connect");
    } catch (e) {
      toasts.error(`Could not connect: ${e}`);
    } finally {
      busy = false;
    }
  }

  // Gmail's step 1: run the OAuth desktop flow (opens the browser), then save the
  // connection with the discovered account email and NO token (the grant lives
  // under the account key in the keyring). Then carry on into step 2, like connect().
  async function connectGmailAccount() {
    if (!canConnectGmail) return;
    busy = true;
    try {
      const email = await connectGmail();
      const conn: Connection = {
        id: effectiveId,
        provider: "gmail",
        label: label.trim() || `Gmail (${email})`,
        baseUrl: null,
        account: email,
      };
      const updated = await saveBriefConnection(brief.path, conn, "");
      projects.upsert(updated);
      toasts.success(`${email} connected ✓`);
      startChoose(conn.id, "connect");
    } catch (e) {
      toasts.error(`Could not connect Gmail: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function addFeed() {
    if (!targetConn) return;
    busy = true;
    try {
      const k = (availableKinds as readonly string[]).includes(kind) ? kind : "tasks";
      // The "Max items" input is type=number, so `limit` can come back as a
      // number (or null) rather than the seeded string — coerce defensively.
      const limitStr = String(limit ?? "").trim();
      const integration: BriefIntegration = {
        connection: targetConn.id,
        kind: k,
        query: query.trim() || null,
        // A Notion "page" feed is a single page — no item cap.
        limit: k === "page" ? null : limitStr ? Number(limitStr) : null,
      };
      const updated = await saveBriefIntegration(brief.path, integration);
      projects.upsert(updated);
      toasts.success(`Pulling ${kindLabel(k, targetConn.provider).toLowerCase()} from ${targetConn.label}`);
      // Warm the panel so the new feed shows data immediately.
      integrations
        .fetch(brief.path, integration.connection, k, integration.query, integration.limit, true)
        .catch(() => {});
      view = "manage";
    } catch (e) {
      toasts.error(`Could not add feed: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function removeFeed(ig: BriefIntegration) {
    try {
      const updated = await deleteBriefIntegration(brief.path, ig.connection, ig.kind, ig.query);
      projects.upsert(updated);
      toasts.push("Feed removed", "info");
    } catch (e) {
      toasts.error(`Could not remove: ${e}`);
    }
  }

  async function removeConn(c: Connection) {
    try {
      const updated = await deleteBriefConnection(brief.path, c.id);
      projects.upsert(updated);
      toasts.push(`Removed ${c.label}`, "info");
    } catch (e) {
      toasts.error(`Could not remove: ${e}`);
    }
  }

  function startEditConn(c: Connection) {
    editingFeedKey = null;
    editingConnId = c.id;
    editConnLabel = c.label;
  }

  async function saveEditConn(c: Connection) {
    const next = editConnLabel.trim();
    if (!next || next === c.label) {
      editingConnId = null;
      return;
    }
    try {
      // Gmail (and any existing connection) keeps its token on an empty string;
      // save_brief_connection upserts by id, so only the label changes.
      const updated = await saveBriefConnection(brief.path, { ...c, label: next }, "");
      projects.upsert(updated);
      editingConnId = null;
      toasts.success("Renamed");
    } catch (e) {
      toasts.error(`Could not rename: ${e}`);
    }
  }

  function startEditFeed(f: BriefIntegration) {
    editingConnId = null;
    editingFeedKey = feedKey(f);
    editFeedQuery = f.query ?? "";
  }

  async function saveEditFeed(f: BriefIntegration, prov: Provider) {
    const next = editFeedQuery.trim();
    // Gmail and Notion require a query (it's the feed's target, not a filter).
    if ((prov === "gmail" || prov === "notion") && !next) {
      toasts.error("A search query is required.");
      return;
    }
    const nextQuery = next || null;
    if (nextQuery === (f.query ?? null)) {
      editingFeedKey = null;
      return;
    }
    try {
      // Feed identity is (connection, kind, query), so an edited query is a new
      // selector: drop the old one, then save the new (same kind + item cap).
      await deleteBriefIntegration(brief.path, f.connection, f.kind, f.query ?? null);
      const updated = await saveBriefIntegration(brief.path, {
        connection: f.connection,
        kind: f.kind,
        query: nextQuery,
        limit: f.limit ?? null,
      });
      projects.upsert(updated);
      editingFeedKey = null;
      toasts.success("Filter updated");
      // Refresh the panel against the new query.
      integrations
        .fetch(brief.path, f.connection, f.kind, nextQuery, f.limit ?? null, true)
        .catch(() => {});
    } catch (e) {
      toasts.error(`Could not update filter: ${e}`);
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

  function goBack() {
    if (view === "connect") view = brief.integrations.length ? "manage" : "pick";
    else if (view === "choose") view = chooseReturn;
    else if (view === "pick") view = "manage";
  }

  let canGoBack = $derived(
    view === "connect" || view === "choose" || (view === "pick" && brief.integrations.length > 0),
  );

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

      {#if view === "manage" || view === "pick"}
        <span
          class="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-[9px] text-white"
          style="background: var(--accent);"
        >
          <Icon name={view === "pick" ? "add_link" : "hub"} size={16} />
        </span>
      {:else if view === "connect"}
        <ProviderTile {provider} size={30} />
      {:else if targetConn}
        <ProviderTile provider={targetConn.provider} size={30} />
      {/if}

      <div class="min-w-0 flex-1">
        <div class="truncate text-[14.5px] font-semibold text-[var(--fg)]">{headerTitle}</div>
        <div class="mt-px truncate text-[11.5px] text-[var(--fg3)]">{headerSub}</div>
      </div>

      {#if view === "connect" || view === "choose"}
        <!-- 2-dot step indicator -->
        <div class="mr-2 flex shrink-0 items-center gap-1.5">
          <span class="h-1 w-[18px] rounded-full" style="background: var(--accent);"></span>
          <span
            class="h-1 w-[18px] rounded-full"
            style="background: {view === 'choose' ? 'var(--accent)' : 'var(--border)'};"
          ></span>
        </div>
      {/if}

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
      {#if view === "manage"}
        <!-- ============ MANAGE: tools grouped by connection ============ -->
        {#each groups as g (g.conn.id)}
          <div class="mb-3 overflow-hidden rounded-[13px] border" style="border-color: var(--border);">
            <!-- Account card head -->
            <div class="flex items-center gap-[10px] px-[13px] py-[11px]" style="background: var(--side-bg);">
              <ProviderTile provider={g.conn.provider} size={28} />
              {#if editingConnId === g.conn.id}
                <input
                  class="h-[28px] min-w-0 flex-1 rounded-[7px] border px-[8px] text-[12.5px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                  style="background: var(--input-bg); border-color: var(--border);"
                  bind:value={editConnLabel}
                  use:focusInput
                  aria-label="Connection label"
                  onkeydown={(e) => {
                    if (e.key === "Enter") saveEditConn(g.conn);
                    else if (e.key === "Escape") editingConnId = null;
                  }}
                />
                <button
                  class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--accent)] transition-colors hover:bg-[var(--hover)]"
                  title="Save"
                  aria-label="Save label"
                  onclick={() => saveEditConn(g.conn)}
                >
                  <Icon name="check" size={16} />
                </button>
                <button
                  class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:text-[var(--fg)]"
                  title="Cancel"
                  aria-label="Cancel rename"
                  onclick={() => (editingConnId = null)}
                >
                  <Icon name="close" size={16} />
                </button>
              {:else}
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[12.5px] font-semibold text-[var(--fg)]">{g.conn.label}</div>
                  <div class="truncate text-[11px] text-[var(--fg3)]">{PROVIDERS[g.conn.provider].label}</div>
                </div>
                <button
                  class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)]"
                  title="Rename"
                  aria-label="Rename {g.conn.label}"
                  onclick={() => startEditConn(g.conn)}
                >
                  <Icon name="edit" size={14} />
                </button>
                <button
                  class="inline-flex h-[26px] shrink-0 items-center gap-[5px] rounded-[7px] border bg-[var(--bg)] px-[9px] text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:bg-[var(--hover)] hover:text-[var(--fg)] disabled:opacity-50"
                  style="border-color: var(--border);"
                  onclick={() => testConn(g.conn)}
                  disabled={testing === g.conn.id}
                >
                  <Icon name="wifi_tethering" size={13} class={testing === g.conn.id ? "spin" : ""} />
                  {testing === g.conn.id ? "Testing…" : "Test"}
                </button>
                <button
                  class="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-lg text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)]"
                  title="Remove tool"
                  aria-label="Remove {g.conn.label}"
                  onclick={() => removeConn(g.conn)}
                >
                  <Icon name="delete" size={15} />
                </button>
              {/if}
            </div>

            {#if g.feeds.length}
              <!-- Nested feeds -->
              {#each g.feeds as f (feedKey(f))}
                <div class="flex items-center gap-[10px] border-t px-[13px] py-[10px]" style="border-color: var(--border);">
                  <Icon name={kindIcon(f.kind, g.conn.provider)} size={15} class="text-[var(--fg3)]" />
                  <span class="shrink-0 text-[12px] font-medium text-[var(--fg)]">{kindLabel(f.kind, g.conn.provider)}</span>
                  {#if editingFeedKey === feedKey(f)}
                    <input
                      class="h-[26px] min-w-0 flex-1 rounded-[6px] border px-[7px] font-mono text-[11px] text-[var(--fg)] outline-none focus:border-[var(--accent)]"
                      style="background: var(--input-bg); border-color: var(--border);"
                      bind:value={editFeedQuery}
                      use:focusInput
                      aria-label="Feed filter"
                      placeholder={g.conn.provider === "gmail" ? "is:unread newer_than:7d" : "Filter"}
                      onkeydown={(e) => {
                        if (e.key === "Enter") saveEditFeed(f, g.conn.provider);
                        else if (e.key === "Escape") editingFeedKey = null;
                      }}
                    />
                    <button
                      class="grid h-[24px] w-[24px] shrink-0 place-items-center rounded-md text-[var(--accent)] transition-colors hover:bg-[var(--hover)]"
                      title="Save"
                      aria-label="Save filter"
                      onclick={() => saveEditFeed(f, g.conn.provider)}
                    >
                      <Icon name="check" size={15} />
                    </button>
                    <button
                      class="grid h-[24px] w-[24px] shrink-0 place-items-center rounded-md text-[var(--fg3)] transition-colors hover:text-[var(--fg)]"
                      title="Cancel"
                      aria-label="Cancel edit"
                      onclick={() => (editingFeedKey = null)}
                    >
                      <Icon name="close" size={15} />
                    </button>
                  {:else}
                    {#if f.query}
                      <span class="truncate rounded-[5px] bg-[var(--code-bg)] px-[6px] py-px font-mono text-[10.5px] text-[var(--fg3)]">{f.query}</span>
                    {/if}
                    <button
                      class="ml-auto grid h-[24px] w-[24px] shrink-0 place-items-center rounded-md text-[var(--fg3)] transition-colors hover:text-[var(--fg)]"
                      title="Edit filter"
                      aria-label="Edit filter"
                      onclick={() => startEditFeed(f)}
                    >
                      <Icon name="edit" size={14} />
                    </button>
                    <button
                      class="grid h-[24px] w-[24px] shrink-0 place-items-center rounded-md text-[var(--fg3)] transition-colors hover:text-[var(--status-blocked)]"
                      title="Remove feed"
                      aria-label="Remove feed"
                      onclick={() => removeFeed(f)}
                    >
                      <Icon name="close" size={15} />
                    </button>
                  {/if}
                </div>
              {/each}
              <div class="border-t border-dashed px-[13px] py-[9px]" style="border-color: var(--border);">
                <button
                  class="inline-flex items-center gap-[5px] text-[11.5px] font-medium text-[var(--accent)] hover:underline"
                  onclick={() => startChoose(g.conn.id, "manage")}
                >
                  <Icon name="add" size={14} /> Add another view from {PROVIDERS[g.conn.provider].label}
                </button>
              </div>
            {:else}
              <!-- THE SAFETY NET: a connected tool with no feed loudly asks for one -->
              <div
                class="flex items-center gap-[11px] border-t px-[13px] py-3"
                style="border-color: var(--border); background: color-mix(in srgb, var(--accent) 9%, transparent);"
              >
                <Icon name="arrow_downward" size={18} class="text-[var(--accent)]" />
                <div class="flex-1">
                  <div class="text-[12.5px] font-semibold text-[var(--fg)]">Almost there — add a feed</div>
                  <div class="mt-px text-[11px] text-[var(--fg2)]">
                    {g.conn.label} is connected, but nothing's being pulled in yet. Choose what to show.
                  </div>
                </div>
                <button
                  class="pulse inline-flex h-[30px] shrink-0 items-center gap-[5px] rounded-lg bg-[var(--accent)] px-3 text-[12px] font-medium text-white transition-[filter] hover:brightness-[1.06]"
                  onclick={() => startChoose(g.conn.id, "manage")}
                >
                  <Icon name="add" size={15} /> Add feed
                </button>
              </div>
            {/if}
          </div>
        {/each}

        <button
          class="inline-flex h-[38px] w-full items-center justify-center gap-[6px] rounded-lg bg-[var(--accent)] text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06]"
          onclick={() => (view = "pick")}
        >
          <Icon name="add" size={16} /> Connect another tool
        </button>

        <p class="mt-[14px] text-[11px] leading-[1.5] text-[var(--fg3)]">
          An <strong class="text-[var(--fg2)]">account</strong> is your sign-in to a tool. A
          <strong class="text-[var(--fg2)]">feed</strong> is one thing it pulls into this brief — like “my open Linear
          issues.” One account can power several feeds.
        </p>
      {:else if view === "pick"}
        <!-- ============ PICK (also the empty state) ============ -->
        <div class="px-2 pb-[18px] pt-2 text-center">
          <div class="mb-1 text-[16px] font-semibold text-[var(--fg)]">Connect a tool to start</div>
          <div class="mx-auto max-w-[44ch] text-[12.5px] leading-[1.5] text-[var(--fg3)]">
            Pick where your tasks live. We'll sign you in, then ask what to pull into this brief.
          </div>
        </div>
        <div class="grid grid-cols-2 gap-[10px]">
          {#each PROVIDER_ORDER as prov (prov)}
            {@const pp = PROVIDERS[prov]}
            <button
              class="group flex items-center gap-[11px] rounded-[12px] border bg-[var(--bg)] p-[13px] text-left transition-all hover:-translate-y-px hover:shadow-[0_4px_14px_rgba(15,30,60,0.08)]"
              style="border-color: var(--border);"
              onclick={() => pickProvider(prov)}
            >
              <ProviderTile provider={prov} size={38} />
              <div class="min-w-0 flex-1">
                <div class="text-[13px] font-semibold text-[var(--fg)]">{pp.label}</div>
                <div class="truncate text-[11px] text-[var(--fg3)]">{pp.blurb}</div>
              </div>
              <Icon name="chevron_right" size={18} class="text-[var(--fg3)]" />
            </button>
          {/each}
        </div>
      {:else if view === "connect"}
        <!-- ============ CONNECT (step 1) ============ -->
        {#if existingForProvider.length}
          <div class="mb-[9px] text-[10px] font-semibold uppercase tracking-[0.07em] text-[var(--fg3)]">
            Use an account you've already connected
          </div>
          <div class="mb-[14px] rounded-[11px] border p-1.5" style="border-color: var(--border);">
            {#each existingForProvider as a (a.id)}
              <button
                class="flex w-full items-center gap-[10px] rounded-lg px-[9px] py-2 text-left transition-colors hover:bg-[var(--hover)]"
                onclick={() => startChoose(a.id, "connect")}
              >
                <ProviderTile provider={a.provider} size={26} />
                <span class="flex-1 truncate text-[12.5px] font-medium text-[var(--fg)]">{a.label}</span>
                <Icon name="chevron_right" size={17} class="text-[var(--fg3)]" />
              </button>
            {/each}
          </div>
          <div class="mb-[14px] flex items-center gap-[10px] text-[10.5px] uppercase tracking-[0.06em] text-[var(--fg3)]">
            <span class="h-px flex-1" style="background: var(--border);"></span>
            or connect a new one
            <span class="h-px flex-1" style="background: var(--border);"></span>
          </div>
        {/if}

        <label class="mb-3 flex flex-col gap-[5px]">
          <span class="text-[11px] text-[var(--fg3)]">Name this account</span>
          <input
            class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
            style="background: var(--input-bg); border-color: var(--border);"
            placeholder={`${meta.label} — work`}
            bind:value={label}
          />
        </label>

        {#if meta.needsBaseUrl}
          <label class="mb-3 flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">
              {provider === "github" ? "Base URL (Enterprise — optional)" : "Base URL"}
            </span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              placeholder={provider === "jira" ? "https://acme.atlassian.net" : "https://github.example.com"}
              bind:value={baseUrl}
            />
          </label>
        {/if}

        {#if meta.needsAccount && provider !== "gmail"}
          <label class="mb-3 flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">
              {provider === "asana" ? "Workspace ID (optional)" : "Account email"}
            </span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              placeholder={provider === "asana" ? "1200000000000000" : "you@acme.com"}
              bind:value={account}
            />
          </label>
        {/if}

        {#if idTaken}
          <!-- Only surfaced on a slug collision — otherwise the id is silent. -->
          <label class="mb-3 flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--status-blocked)]">Account id (in use — pick another)</span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] font-mono text-[12px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--status-blocked);"
              placeholder={slugify(label) || provider}
              bind:value={id}
            />
            <span class="text-[10.5px] text-[var(--status-blocked)]">
              <code>{effectiveId}</code> is already used on this brief.
            </span>
          </label>
        {/if}

        {#if provider === "gmail"}
          <!-- Gmail signs in via OAuth (opens your browser); no token to paste. -->
          <button
            class="inline-flex h-[38px] w-full items-center justify-center gap-[7px] rounded-lg bg-[var(--accent)] text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
            onclick={connectGmailAccount}
            disabled={!canConnectGmail}
          >
            <Icon name="open_in_new" size={15} />
            {busy ? "Waiting for Google…" : "Connect Google account"}
          </button>
          <p class="mt-2 text-[10.5px] leading-[1.5] text-[var(--fg3)]">
            Opens Google sign-in in your browser. WAID requests <strong class="text-[var(--fg2)]">read-only</strong>
            Gmail access; the grant is stored in your OS keychain — never written to the brief. Set up your own
            Google OAuth client in Settings first (one-time).
          </p>
        {:else}
          <label class="flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">API token</span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              type="password"
              autocomplete="off"
              placeholder="Paste your token"
              bind:value={token}
            />
            <span class="text-[10.5px] text-[var(--fg3)]">
              <Icon name="lock" size={12} class="-mt-px mr-0.5" />Stored in your OS keychain — never written to the brief.
            </span>
          </label>
        {/if}
      {:else if view === "choose" && targetConn}
        <!-- ============ CHOOSE (step 2) ============ -->
        <div
          class="mb-4 flex items-center gap-[11px] rounded-[12px] border p-3"
          style="border-color: var(--border); background: var(--side-bg);"
        >
          <ProviderTile provider={targetConn.provider} size={34} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px] font-semibold text-[var(--fg)]">{targetConn.label}</div>
            <div class="truncate text-[11px] text-[var(--fg3)]">Connected ✓ — now pick what to show in this brief</div>
          </div>
        </div>

        {#if availableKinds.length > 1}
          <div class="mb-3 flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">{isNotion ? "This Notion URL is a…" : "Pull in"}</span>
            <div class="inline-flex gap-[3px] self-start rounded-[9px] p-[3px]" style="background: var(--chip-bg);">
              {#each availableKinds as k (k)}
                <button
                  class="inline-flex items-center gap-[5px] rounded-[7px] px-[11px] py-1 text-[11.5px] font-medium transition-colors {kind === k
                    ? 'bg-[var(--bg)] text-[var(--fg)] shadow-[0_1px_2px_rgba(15,30,60,0.1)] dark:bg-[var(--accent)] dark:text-white'
                    : 'text-[var(--fg2)]'}"
                  onclick={() => (kind = k)}
                >
                  <Icon name={kindIcon(k, targetConn.provider)} size={14} /> {kindLabel(k, targetConn.provider)}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="grid gap-[10px] {isNotionPage ? 'grid-cols-1' : 'grid-cols-[1fr_88px]'}">
          <label class="flex flex-col gap-[5px]">
            <span class="text-[11px] text-[var(--fg3)]">
              {#if isNotionPage}Notion page URL{:else if isNotion}Notion database URL{:else if isGmail}Gmail search query{:else}Filter (optional){/if}
            </span>
            <input
              class="h-[34px] rounded-[9px] border px-[10px] font-mono text-[11.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
              style="background: var(--input-bg); border-color: var(--border);"
              placeholder={isNotionPage ? "https://www.notion.so/My-Page-…" : PROVIDERS[targetConn.provider].queryPlaceholder}
              bind:value={query}
            />
          </label>
          {#if !isNotionPage}
            <label class="flex flex-col gap-[5px]">
              <span class="text-[11px] text-[var(--fg3)]">Max items</span>
              <input
                class="h-[34px] rounded-[9px] border px-[10px] text-[12.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                type="number"
                min="1"
                placeholder="20"
                bind:value={limit}
              />
            </label>
          {/if}
        </div>
        {#if isGmail}
          <div class="mt-[10px] flex flex-col gap-[6px]">
            <span class="text-[10.5px] text-[var(--fg3)]">Templates — click to use, then tweak</span>
            <div class="flex flex-wrap gap-[6px]">
              {#each GMAIL_TEMPLATES as t (t.label)}
                <button
                  type="button"
                  class="rounded-full border px-[9px] py-[3px] text-[11px] transition-colors {query === t.query
                    ? 'border-[var(--accent)] bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-[var(--fg)]'
                    : 'text-[var(--fg2)] hover:border-[var(--accent)] hover:text-[var(--fg)]'}"
                  style="border-color: {query === t.query ? 'var(--accent)' : 'var(--border)'};"
                  title={t.query}
                  onclick={() => (query = t.query)}
                >
                  {t.label}
                </button>
              {/each}
            </div>
          </div>
          <div class="mt-[10px] flex flex-col gap-[6px]">
            <span class="text-[10.5px] text-[var(--fg3)]">Or describe it — AI builds the search</span>
            <div class="flex gap-[6px]">
              <input
                class="h-[32px] min-w-0 flex-1 rounded-[8px] border px-[9px] text-[11.5px] text-[var(--fg)] outline-none transition-[border-color,box-shadow] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
                style="background: var(--input-bg); border-color: var(--border);"
                placeholder="unread from my manager this week"
                bind:value={aiPrompt}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    generateQuery();
                  }
                }}
              />
              <button
                type="button"
                class="inline-flex h-[32px] shrink-0 items-center gap-[5px] rounded-[8px] border px-[11px] text-[11.5px] font-medium text-[var(--fg2)] transition-colors hover:border-[var(--accent)] hover:text-[var(--fg)] disabled:opacity-50"
                style="border-color: var(--border);"
                onclick={generateQuery}
                disabled={aiBusy || !aiPrompt.trim()}
              >
                <Icon name={aiBusy ? "progress_activity" : "auto_awesome"} size={14} class={aiBusy ? "spin" : ""} />
                {aiBusy ? "Generating…" : "Generate"}
              </button>
            </div>
          </div>
        {/if}
        <span class="mt-[10px] block text-[11px] text-[var(--fg3)]">
          {#if isNotionPage}
            Its text becomes context for this brief's synthesised Current State. Share the page with your integration first.
          {:else if isNotion}
            Paste a database you've shared with this integration. Its rows show up as items.
          {:else if isGmail}
            A Gmail search — operators like <code>from:</code>, <code>subject:</code>, <code>label:</code>,
            <code>newer_than:14d</code>, <code>is:unread</code>, <code>to:me</code>. Matching emails show up as items
            (read-only; subjects &amp; snippets only).
          {:else}
            Leave the filter blank to pull {PROVIDERS[targetConn.provider].blurb.toLowerCase()}.
          {/if}
        </span>
      {/if}
    </div>

    <!-- Footer (only the two wizard steps have one; Gmail's connect uses the
         in-body OAuth button, so it has no footer). -->
    {#if view === "connect" && provider !== "gmail"}
      <div class="flex items-center justify-between gap-3 border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <span class="inline-flex items-center gap-[5px] text-[10.5px] text-[var(--fg3)]">
          <Icon name="schedule" size={13} /> Takes ~10 seconds
        </span>
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={connect}
          disabled={!canConnect}
        >
          {#if busy}
            Connecting…
          {:else}
            Connect &amp; continue <Icon name="arrow_forward" size={14} />
          {/if}
        </button>
      </div>
    {:else if view === "choose"}
      <div class="flex items-center justify-end gap-3 border-t px-[18px] py-[13px]" style="border-color: var(--border);">
        <button
          class="inline-flex h-[32px] items-center gap-[5px] rounded-lg bg-[var(--accent)] px-4 text-[12.5px] font-medium text-white transition-[filter] hover:brightness-[1.06] disabled:opacity-50"
          onclick={addFeed}
          disabled={!canAddFeed}
        >
          <Icon name="check" size={14} /> Add to brief
        </button>
      </div>
    {/if}
  </div>
</div>
