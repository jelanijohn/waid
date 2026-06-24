# WAID — "What Am I Doing?" — Project Context Summary

> A portable summary of the **WAID** desktop project dashboard, intended to be
> dropped into a Claude project's knowledge base as context.

> [!IMPORTANT]
> **Disambiguation — read this first.** WAID is **not** *What's Next?*. They are
> two separate repos with different stacks and purposes:
> - **What's Next?** — a **Flutter/Dart mobile** app. *Micro* scope: "what single
>   task should I work on right now?" (weighted random task picker + timers).
> - **WAID** — a **Tauri 2 + SvelteKit + Tailwind v4 desktop** app. *Macro*
>   scope: "what's the state of all my projects?" (a dashboard over markdown
>   project briefs).
>
> If a prompt is about WAID, the frontend is **web tech (Svelte 5 + TypeScript +
> Tailwind CSS)** running in a Tauri webview, with a **Rust** backend — *not*
> Flutter. The filesystem lives at `…/Projects/waid`.

---

## 1. Product Concept

**WAID — "What Am I Doing?"** is a **local-first desktop dashboard** for managing
active projects. It is the **macro companion** to *What's Next?*: where What's
Next answers "what task now?", WAID answers **"what's the state of all my
projects?"**

It lists your projects, renders each project's context **brief**, and lets you
launch into work — open a Claude project, open a repo, fire a webhook, or jot a
quick note. It can **pull live state** from a project's integrations (GitHub
deterministically, plus first-class **project-management connectors**: Linear,
Jira, Asana, GitHub, **Notion**, **Gmail**, **Slack**, and **Figma**, plus a local
**NeuroSkill** EEG link that writes a deterministic `## Mind State` body region) and, optionally,
**synthesize**
prose with a local or cloud LLM — both a per-brief status summary and a
cross-brief **morning briefing**. New projects can be **bootstrapped** into an initial brief from a
folder, a GitHub repo, a guided interview, or a pasted prompt. It is a **personal
tool, not a team tool**.

- **Author:** Jelani John
- **Version:** 0.1.0
- **License:** MIT
- **Scope:** single-user, local, **desktop-only by design**. Mobile/web, auth,
  multi-user, and sync are explicitly **not** planned.

---

## 2. How It Works

### Data is just markdown files

Each project is **one plain `.md` file** with **YAML frontmatter** (metadata) +
a **markdown body** (the brief). WAID reads every `.md` in a configured **briefs
directory**.

- **Default briefs dir:** `~/WAID/briefs`. On first run, if empty, it is
  **seeded** with sample briefs (bundled into the binary via `include_str!`) so
  there's real content immediately. The seeds are **generic `sample-*.md`
  placeholders** (`sample-web-app`, `sample-mobile-app`, `sample-side-project`,
  `sample-research`, `sample-archived`), safe to ship.
- **Configurable:** a **Change** link in the sidebar repoints it at any folder
  (e.g. an Obsidian vault). The choice persists to `settings.json` in the app
  config dir.
- **Recursive scan.** `list_briefs` walks the directory recursively and skips
  dot-entries (`.obsidian/`, `.trash/`, `.git/`, …), so the briefs dir can be an
  Obsidian vault or any subfolder of one.
- **Obsidian-compatible by design** — standard frontmatter + standard markdown,
  no proprietary format.

### Obsidian vault integration

When the briefs folder lives inside an Obsidian vault, WAID adds:

- **`[[wikilinks]]`** in the brief body — links to other briefs are clickable
  (they select that brief); dangling links render muted + dashed. Aliases
  (`[[note|label]]`) and heading anchors (`[[note#section]]`) are supported. The
  wiring is **purely frontend** (computed from the loaded brief list:
  `resolveWikilink` / `nameIndex` / `backlinksFor` / `obsidianUri`).
- **Backlinks** — a "Linked from" list of every other brief that wikilinks to
  the current one.
- **Open in Obsidian** — an `obsidian://open?vault=<name>&file=<path>` deep link.
  Requires a one-time setup (the vault must have been opened in Obsidian once,
  and its name must match its folder name); WAID can't auto-register it.
- `get_vault_info` walks up from the briefs dir to the nearest `.obsidian/` and
  returns `{ isVault, name, root }`. There's also an `is_wsl` command + README
  guidance for the Windows-Obsidian-over-WSL case.

### App-owned regions (sync + synthesis write here)

WAID writes machine-managed content into **marker-delimited regions** of the
body. Markers are HTML comments (render to nothing; files stay Obsidian-native).
Everything outside the markers — your prose, `## Captures`, all frontmatter
except the keys noted below — is preserved **byte-for-byte**.

| Region | Markers | Owner | Producer |
|---|---|---|---|
| `## Activity` | `waid:sync:start/end` | App | Brief sync — deterministic, no LLM |
| `## Current State` | `waid:state:start/end` | App | Synthesis — LLM, regenerated wholesale (also **seeded by bootstrap**) |
| `## Open Questions` (inner block) | `waid:questions:start/end` (nested) | Shared | Synthesis — LLM regenerates inner block only (also **seeded by bootstrap**) |
| `## Captures` | — | Human | `append_capture`, append-only |
| frontmatter | — | Human (mostly) | edit mode; `last_opened`, `connections`, `integrations` are key-spliced (see below) |

**Frontmatter writes are surgical.** The body-marker round-trip preserves
frontmatter verbatim, but a few commands deliberately *do* edit specific keys by
parsing the frontmatter into a `serde_yaml::Mapping`, splicing only their own
key, and re-emitting — every other key (and the body) survives. `touch_brief`
does this for `last_opened`; the integration commands
(`save_brief_connection` / `save_brief_integration` and their deletes) do it for
the `connections` and `integrations` keys. Nothing else in frontmatter is ever
machine-written.

### Brief bootstrap (initial-brief generation)

A freshly-created **stub brief** (just an H1 + "Project context goes here.",
detected by `isStubBrief` in `bootstrap.ts`) surfaces a **"Generate the initial
brief"** call-to-action in `ProjectDetail`. `BootstrapModal.svelte` offers four
methods, each of which returns a **proposed raw file string** (frontmatter +
body) that lands in **edit mode** — nothing is written to disk until the human
hits Save (the same commit gate every write path uses):

- **Point at a folder / repo** (`bootstrap_from_folder`) — reads the project's
  README + manifest + file tree.
- **GitHub URL** (`bootstrap_from_github`) — pulls the repo's description, topics,
  and README.
- **Guided interview** (`bootstrap_from_answers`) — three questions (`summary`,
  `goal`, `notes`), composed into a brief **locally even with no provider**.
- **Paste a prompt** — copies a handoff prompt (`pastePromptTemplate` in
  `bootstrap.ts`) to use with any AI; the pasted result is run through
  `normalize_bootstrap_paste` on import.

Deterministic metadata is always produced; an **AI-drafted body** is added only
when an LLM provider is configured (otherwise a stub / locally-composed body).
The modal reorders its method list to recommend GitHub/folder first when the
brief already carries a GitHub link, and paste/interview first otherwise.

Crucially, bootstrap **seeds `## Current State` and `## Open Questions` through
the same `waid:state` / `waid:questions` markers the synthesis agent owns** (via
`upsert_marked_block` / `merge_open_questions`). The markers — not a plain
heading — are the differentiator: a later **Refresh** finds them and regenerates
in place instead of appending a duplicate block. The paste path normalizes plain
`## Current State` / `## Open Questions` headings into those markers on import
for the same reason (`normalize_owned_regions`, idempotent). All bootstrap
evidence (README, manifest, pasted text, interview answers) is treated as
**data, never instructions**.

### Backend commands (Tauri `invoke`)

Registered in `src-tauri/src/lib.rs`, implemented in `commands.rs`:

| Command | Purpose |
|---|---|
| `list_briefs` | List/parse all `.md` briefs (recursive), sorted by `last_opened` desc, then name |
| `read_brief` | Read + parse a single brief by absolute path |
| `save_brief` | Overwrite a brief's raw contents (edit-mode save) |
| `touch_brief` | Update/insert `last_opened`, preserving all other frontmatter keys |
| `append_capture` | Append a timestamped note under a `## Captures` heading (creates it if absent) |
| `fire_webhook` | Async HTTP request (GET/POST/PUT/PATCH/DELETE), JSON body, custom headers (one value may interpolate a `{{secret}}` loaded internally from the keyring), returns status for toast |
| `save_brief_webhook` / `delete_brief_webhook` | Add/update or remove a per-brief webhook (header shapes → frontmatter, secret → keyring keyed `whook:<brief-path>:<id>`); identity = stable slug `id` |
| `sync_brief` / `sync_all` | Refresh the `## Activity` block from a brief's GitHub link(s) + explicit `sources` (deterministic) |
| `synthesize_brief` / `synthesize_all` | LLM synthesis of `## Current State` + `## Open Questions` (no-op error if no provider) |
| `list_ollama_models` | List models from an Ollama server (for the settings dropdown) |
| `get_llm_settings` / `set_llm_settings` | Read / persist provider + model config (never secrets) |
| `get_briefs_dir` / `set_briefs_dir` | Read / repoint + persist the briefs directory |
| `get_vault_info` | Whether the briefs dir is in an Obsidian vault + vault name/root |
| `is_wsl` | Detect WSL (for Obsidian deep-link guidance) |
| `create_brief` | Scaffold a new brief from a name (slugified filename + starter frontmatter) |
| `set_secret` / `get_secret` / `delete_secret` / `has_secret` | OS-keyring secret storage |
| `save_brief_connection` / `delete_brief_connection` | **(PM)** Add/update or remove a per-brief connection (metadata → frontmatter, token → keyring) |
| `save_brief_integration` / `delete_brief_integration` | **(PM)** Add/update or remove an integration *selector* on a brief (identity = connection id + kind + query) |
| `test_brief_connection` | **(PM)** Verify a brief connection's saved token against its provider (`validate`) |
| `connect_gmail` | **(PM, Gmail)** Run the Google OAuth desktop loopback + PKCE flow for one Gmail account (opens the browser, captures consent), store the account-scoped grant in the keyring (`gmail.oauth:<email>`), return the connected email |
| `fetch_integration` | **(PM)** Fetch live items for one of a brief's selectors; token loaded internally from the keyring |
| `sync_mind_state` | **(NeuroSkill)** Regenerate a brief's deterministic `## Mind State` region (marker `waid:mind`) from NeuroSkill's local SQLite (read-only); mirrors `sync_brief`. No LLM |
| `mark_brief_session` | **(NeuroSkill)** Fire a `waid:brief=<slug>:(start\|end)` session label via an authenticated `POST` to the daemon's local HTTP API (bearer token from the daemon's `auth.token`; the only write). Best-effort; a no-op for briefs without a NeuroSkill connection |
| `digest_integrations` | **(PM, LLM)** Prose digest of *one* brief's live items (display-only; Notion `page` + NeuroSkill `mind` feeds excluded) |
| `morning_briefing` | **(PM, LLM)** Cross-brief "morning briefing" over every brief's live items (Notion `page` + NeuroSkill `mind` feeds excluded) |
| `generate_gmail_query` | **(PM, Gmail, LLM)** Turn a plain-English description into a single Gmail search query via the synthesis provider; display-only (the result drops into a feed's `query` field — nothing fetched or written) |
| `generate_slack_query` | **(PM, Slack, LLM)** Turn a plain-English description into a single Slack search query via the synthesis provider; display-only, same shape as `generate_gmail_query` |
| `bootstrap_from_folder` | **(bootstrap)** Draft an initial brief from a local folder/repo (README + manifest + file tree); returns a proposed raw file, never writes |
| `bootstrap_from_github` | **(bootstrap)** Draft an initial brief from a GitHub repo URL (description/topics/README) |
| `bootstrap_from_answers` | **(bootstrap)** Compose an initial brief from a 3-question guided interview (composes locally without a provider) |
| `normalize_bootstrap_paste` | **(bootstrap)** Lift a pasted brief's plain `## Current State` / `## Open Questions` into the app-owned marker regions; pure transform, no disk write |

Frontmatter parsing handles BOM and CRLF, recognises leading/closing `---`, and
falls back to a humanised filename when `name` is missing. **Edits round-trip the
entire raw file** (`split_for_body_edit` gives a lossless `prefix + body` split,
re-attached verbatim on every write), so unknown/extra frontmatter keys are never
mangled or reordered.

---

## 3. Data Model

TypeScript types in `src/lib/types.ts` mirror the Rust structs
(`#[serde(rename_all = "camelCase")]`):

```ts
interface Brief {
  path: string;          // absolute path to the .md file
  fileName: string;
  name: string;
  status?: string | null;        // free-form; rendered as a pill
  description?: string | null;
  tags: string[];
  links: Link[];                 // { label, url } — open in default browser
  webhooks: Webhook[];           // { id (slug), label, url, method (default POST), body?, headers? }
  sources: SyncSource[];         // explicit (non-GitHub) sync sources
  connections: Connection[];     // this brief's PM connections (metadata; never tokens)
  integrations: BriefIntegration[]; // PM selectors (connection id + kind + query; never tokens)
  lastOpened?: string | null;
  lastSynced?: string | null;    // when the Activity block was last written (read from body)
  body: string;                  // markdown after frontmatter (rendered)
  raw: string;                   // entire file incl. frontmatter (edit mode)
}
interface SyncSource {           // non-GitHub sync source
  label: string;
  url: string;
  method?: string | null;
  fields: Record<string, string>;   // flat json_path -> label extraction map
}
interface SyncOutcome { path: string; name: string; ok: boolean; error?: string | null; }
interface WebhookResult { status: number; ok: boolean; body: string; }
interface LlmSettings {          // provider + model config; secrets live in keyring
  llmProvider?: string | null;   // "ollama" | "anthropic" | null (null = disabled)
  ollamaUrl?: string | null;
  ollamaModel?: string | null;
  anthropicModel?: string | null;
}
interface VaultInfo { isVault: boolean; name?: string | null; root?: string | null; }
interface BootstrapAnswers {     // guided-interview answers for brief bootstrap
  summary: string;               // "What is this project?" (required)
  goal?: string | null;          // "What's the goal / definition of done?"
  notes?: string | null;         // "Anything else / current state?" → seeds Current State
}

// --- PM integrations -------------------------------------------------------
type Provider = "linear" | "jira" | "asana" | "github" | "notion" | "gmail" | "slack" | "figma" | "neuroskill";
interface Connection {           // account-level metadata; token lives in the keyring
  id: string;                    // stable slug, e.g. "linear-personal"; part of the keyring key
  provider: Provider;
  label: string;
  baseUrl?: string | null;       // Jira cloud instance / GitHub Enterprise base URL
  account?: string | null;       // e.g. a Jira email — or the OAuth'd Gmail address; never the token
  repos?: string[] | null;       // GitHub only: owner/name repos this connection's feeds are scoped to (required for GitHub)
  wsUrl?: string | null;         // NeuroSkill only: daemon endpoint for the label write (default http://127.0.0.1:18444; ws:// accepted)
  dataDir?: string | null;       // NeuroSkill only: dir holding activity.sqlite / labels.sqlite (default WSL-translated AppData)
  tokenPath?: string | null;     // NeuroSkill only: path to the daemon's auth.token file (default OS …/skill/daemon/auth.token); read at call time, not stored
}
interface BriefIntegration {     // a brief's selector referencing one of its connections
  connection: string;            // -> Connection.id
  kind: string;                  // "tasks" | "notifications" | "pulls" | "commits" (GitHub) | "page" (Notion) | "email" (Gmail) | "messages" (Slack) | "comments" (Figma) | "mind" (NeuroSkill); defaults to tasks
  query?: string | null;         // part of the feed's identity (Gmail: a required search string)
  limit?: number | null;
}
interface IntegrationItem {      // normalized task/notification from any provider
  id: string; title: string; url: string;
  status?: string | null; assignee?: string | null;
  updatedAt?: string | null;     // ISO; render with time.ts
  kind: string;                  // "task" | "notification" | "commit"
  meta?: Record<string, string>; // provider extras (priority, project, …)
}
interface IntegrationSummary {   // local (no-LLM) rollup
  total: number;
  byStatus: Record<string, number>;
  overdue?: number | null;       // null in v1 (no due-date signal)
  updatedRecently?: number | null; // items updated within the last 7 days
}
interface IntegrationFetch { items: IntegrationItem[]; fetchedAt: string; summary: IntegrationSummary; }
```

**Feed identity is `(connection, kind, query)`.** A single connection can host
several feeds of the same kind pointed at different targets — e.g. two Notion
databases, or a Notion database *and* a project page on one Notion connection.
`save_brief_integration` matches on all three (a new query **appends** rather than
overwriting a sibling), and `delete_brief_integration` takes an optional `query`
to disambiguate. The frontend cache (`stores/integrations.svelte.ts`) and Svelte
`{#each}` keys are likewise keyed by `path + connection + kind + query`.

### Brief file format

```markdown
---
name: Sample Web App
status: active            # active | paused | blocked | archived (free-form pill)
description: Example project brief
tags: [sample, web, example]
links:
  - label: Claude Project
    url: https://claude.ai/project/xxx
  - label: GitHub
    url: https://github.com/...
webhooks:
  - id: deploy-staging    # stable slug; scopes the keyring secret, survives relabel
    label: Deploy staging
    url: https://...
    method: POST          # defaults to POST if omitted
    body: '{"env":"staging"}'   # optional; sent as JSON
    headers:              # optional; one value may interpolate {{secret}} from the keyring
      - name: Authorization
        value: "Bearer {{secret}}"
connections:              # PM connections owned by THIS brief (metadata only)
  - id: linear-personal
    provider: linear
    label: Linear (personal)
  - id: notion-work
    provider: notion
    label: Notion
  - id: gmail-personal
    provider: gmail       # OAuth, not a token; `account` is set by the connect flow
    label: Gmail
    account: me@gmail.com
  - id: slack-work
    provider: slack       # token-paste (xoxp- user token, search:read scope)
    label: Slack
  - id: github-work
    provider: github
    label: GitHub
    repos:                # REQUIRED for GitHub — scopes every feed to these repos
      - owner/waid
integrations:             # selectors referencing the connections above
  - connection: linear-personal
    kind: tasks
    query: "assignee:me"
    limit: 10
  - connection: github-work
    kind: commits         # commits matching a search → panel items (repo-scoped)
    query: "author:@me"
  - connection: notion-work
    kind: tasks           # a Notion database's rows → panel items
    query: "https://www.notion.so/My-DB-…"
  - connection: notion-work
    kind: page            # a Notion page → synthesis evidence (not a panel feed)
    query: "https://www.notion.so/Project-Home-…"
  - connection: gmail-personal
    kind: email           # recent Gmail matches → panel items (required search query)
    query: "from:acme.com newer_than:14d"
  - connection: slack-work
    kind: messages        # search.messages results → panel items (required search query)
    query: "in:#waid after:2026-06-01"
last_opened: 2026-05-31T10:00:00Z
---

# Project context
The full markdown brief lives here.

## Captures
<!-- human-owned, append-only notes -->

## Activity
<!-- waid:sync:start --> … managed by sync … <!-- waid:sync:end -->
```

---

## 4. Tech Stack

- **Desktop shell + backend:** [Tauri 2](https://v2.tauri.app/) (Rust)
- **Frontend:** [SvelteKit](https://svelte.dev/) — **Svelte 5 (runes)**,
  **TypeScript**, **SPA mode** (`adapter-static`, `ssr = false`)
- **Styling:** [Tailwind CSS v4](https://tailwindcss.com/) (via
  `@tailwindcss/vite`), **class-based dark mode** (toggle `dark` on `<html>`)
- **Markdown:** `marked` + `DOMPurify`
- **Build tooling:** Vite 6, **pnpm** (a `pnpm-workspace.yaml` is present at root)
- **Rust deps:** `serde`, `serde_json`, `serde_yaml`, `chrono`, `reqwest`
  (rustls-tls), `keyring` (OS secret store), `async_trait` (LLM provider trait),
  `rusqlite` (**bundled** — read-only NeuroSkill SQLite; compiles SQLite in-tree,
  so a C compiler is needed at build time); Tauri plugins: `opener`, `dialog`,
  `global-shortcut`
- **LLM providers (optional):** local **Ollama** (HTTP) and **Anthropic**
  (cloud). Used for brief synthesis, the PM integration digest / morning
  briefing, *and* the AI body in brief bootstrap. No in-process inference.
  Defaults: Ollama `http://localhost:11434`; Anthropic model
  `claude-haiku-4-5-20251001`.
- **PM integrations:** plain JSON over the shared `reqwest` client — **no
  provider SDKs, no GraphQL client**. One submodule per provider.
- **Data:** plain `.md` files on disk — no server, no DB, no auth

### Structure

```
waid/
├── src/                          # SvelteKit frontend
│   ├── app.html
│   ├── app.css                   # Tailwind import + "Tidewater · Refined" tokens + hand-rolled .markdown styles
│   ├── routes/                   # +layout(.ts/.svelte), +page (main two-pane view)
│   └── lib/
│       ├── components/           # Titlebar, AppMenu, Sidebar, ProjectDetail, MarkdownView,
│       │                         #   QuickCapture, StatusPill, Toasts, Icon, BrandMark,
│       │                         #   CredentialHelp, IntegrationPanel, IntegrationsModal,
│       │                         #   WebhooksModal, BriefingModal, BootstrapModal, ProviderTile,
│       │                         #   ResizeHandles
│       ├── stores/               # projects / settings / toasts / integrations (all .svelte.ts runes)
│       ├── tauri.ts              # wrappers around invoke / plugins / secrets / PM + bootstrap commands
│       ├── credentialHelp.ts     # per-credential setup steps + scopes + docs links (CredentialHelp data)
│       ├── types.ts              # Brief / Link / Webhook / SyncSource / SyncOutcome / LlmSettings /
│       │                         #   VaultInfo / BootstrapAnswers / Provider / Connection /
│       │                         #   BriefIntegration / Integration*
│       ├── status.ts             # status order + color/label palette (CSS vars)
│       ├── markdown.ts           # marked + DOMPurify (+ wikilink rendering)
│       ├── bootstrap.ts          # paste-a-prompt template + stub-brief detection (frontend-only)
│       ├── providers.ts          # PM-provider display metadata (brand/monogram/blurb/form needs + kind helpers)
│       └── time.ts               # relative-time helper
├── src-tauri/                    # Rust backend
│   └── src/
│       ├── main.rs               # calls run()
│       ├── lib.rs                # plugin + command registration, global shortcut, window chrome
│       ├── commands.rs           # briefs · webhooks · sync · LLM synthesis · keyring · settings ·
│       │                         #   PM connections/selectors/digests · mind state · brief bootstrap
│       ├── provider/             # PM-integrations module (pure, network-only)
│       │   ├── mod.rs            # shared model (Connection/BriefIntegration/IntegrationItem/…),
│       │   │                     #   fetch/validate dispatch, local summarize rollup
│       │   ├── linear.rs         # one submodule per provider; each has a pure, fixture-tested
│       │   ├── jira.rs           #   map_* response -> IntegrationItem mapper
│       │   ├── asana.rs
│       │   ├── github.rs
│       │   ├── notion.rs         # database rows (fetch) + page-as-evidence (fetch_page_text)
│       │   ├── gmail.rs          # recent emails (metadata-only) → items; pure, takes a bearer token
│       │   ├── slack.rs          # search.messages → items; token-paste, check_ok maps 200-OK errors
│       │   └── figma.rs          # one file's comments → items; token-paste (X-Figma-Token), optional @-me filter
│       └── neuroskill/           # NeuroSkill EEG → ## Mind State (local SQLite read + HTTP write; NOT in provider/)
│           ├── mod.rs            # public API: read-only DB opener, compute/render mind state, http_base/token, read-scope guard
│           ├── aggregate.rs      # pure, fixture-tested rollup: labels→intervals, window filter, epochs→MindState
│           ├── labels.rs         # read-only `labels` query + pure waid:brief=<slug> marker parsing
│           ├── eeg.rs            # read-only `eeg_timeseries` window query + pure metric-JSON extraction
│           └── client.rs         # authenticated HTTP `label` write (reqwest POST + bearer; the only write)
├── briefs/                       # sample briefs (dev + bundled seed; generic placeholders)
├── CLAUDE.md                     # guidance for Claude Code working in the repo
└── README.md
```

> **Doc note.** Earlier standalone planning docs (`waid-agent-spec.md`,
> `waid-pm-integrations-plan.md`, `waid-bootstrap-owned-regions-fix.md`) are no
> longer needed at the repo root — that guidance now lives in `CLAUDE.md` and the
> code comments. CLAUDE.md's "Architecture" section documents the `provider/`
> module in detail.

### Run / build

```bash
pnpm install
pnpm tauri dev      # desktop app with hot reload
pnpm tauri build    # native bundle → src-tauri/target/release/bundle
pnpm check          # svelte-kit sync && svelte-check (frontend type-check)
cargo test          # backend unit tests (from src-tauri/)
```

`pnpm dev` (Vite alone, port 1420) runs the frontend in a browser, but Tauri
`invoke` calls fail there — use `pnpm tauri dev` for anything touching the
backend. (Under WSL, `pnpm tauri:wsl` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`.)
No frontend test suite or linter beyond `svelte-check`. The Rust side has
substantial unit coverage (frontmatter splitting, marker upsert, sync rendering,
synthesis apply + injection-containment, bootstrap apply + owned-region seeding +
paste normalization, integration frontmatter round-trips incl. the
`(connection, kind, query)` identity, Notion id/URL extraction + page mapping,
keyring key scoping); real keyring and live-Notion round-trip tests are
`#[ignore]`d (they need a running Secret Service / keychain daemon, or a real
shared Notion token).

Prerequisites: Node 18+, pnpm (`corepack enable pnpm`), Rust stable (rustup),
and Tauri platform build deps (the README's `apt` line on Debian/Ubuntu).

---

## 5. PM Integrations (shipped)

A major addition since the early versions: first-class **project-management
connectors**, living in `src-tauri/src/provider/`.

### Design

- **Providers:** **Linear, Jira, Asana, GitHub, Notion, Gmail, Slack, Figma** —
  dispatched on
  `Connection.provider`. Each provider is a plain authenticated JSON request,
  normalised into a common `IntegrationItem` (id, title, url, status, assignee,
  `updatedAt`, kind, free-form `meta`). Each submodule has a pure,
  fixture-tested `map_*` response→item mapper. (Gmail is the lone OAuth provider;
  see its subsection below.) **NeuroSkill** is a ninth provider in the enum but
  the odd one out: it isn't a panel feed and isn't dispatched through this
  network `fetch`/`validate` — it reads **local SQLite** and writes a body
  region, so it lives in its own `src-tauri/src/neuroskill/` module (see its
  subsection below).
- **Connections are per-brief.** A connection's **non-secret metadata** (id,
  provider, label, base URL, account) lives in *that brief's* `connections:`
  frontmatter and round-trips via the file like `links` / `webhooks`. The
  **token** lives in the OS keyring keyed `bconn:<brief-path>:<connection-id>`,
  so two briefs can reuse the same connection id without colliding. (Renaming a
  brief file orphans its tokens — harmless; you just re-enter them.)
- **Selectors.** A brief's `integrations:` entries each reference one of its own
  connections by id, plus a `kind` (`tasks` | `notifications` | `pulls` |
  `commits` | `page` | `email` | `messages`), optional
  `query` and `limit`. **Feed identity is `(connection, kind, query)`**, so one
  connection can carry multiple feeds (e.g. several Notion databases/pages).
- **The `provider` module is pure / network-only.** It never touches disk or the
  keyring — `commands.rs` loads the token and passes it in. The caller supplies
  `fetched_at` (not read from the clock inside) so fetches stay deterministic.
- **Local rollup, no LLM:** `summarize` computes
  `{ total, byStatus, updatedRecently }` over the items (overdue is `null` in
  v1 — no due-date signal yet). Rendered as a one-line summary in the panel header.
- **Strictly additive / bounded blast radius.** A failed fetch is a toast and a
  panel error state — **never** a write into the `.md`. The brief renders fully
  regardless.

### GitHub connector (REST + search, repo-scoped)

GitHub is a `Bearer <token>` (PAT) REST connector with **four feed kinds**, and
the only provider that **requires repo scoping** on its connection.

- **Two REST feeds.** `tasks` lists open issues & PRs assigned to the user
  (`/issues?filter=assigned`); `notifications` lists unread notifications. Both
  are cross-repo endpoints, so WAID filters their items down to the connection's
  `repos` after mapping (`retain_by_repos`).
- **Two search feeds.** `pulls` lists PRs matching a `query` (default `is:pr
  is:open author:@me`, via `/search/issues` — search-issue items share the issue
  shape, so `map_issue` applies); `commits` lists commits matching a `query`
  (default `author:@me`, via `/search/commits`, mapped by the fixture-tested
  `map_commit` — subject → title, short SHA + repo in `meta`, author login → the
  assignee slot). They're **independent feeds**, not a toggle: a brief can carry
  either, both, or neither.
- **Required repo scoping.** `/search/...` returns *public* matches regardless of
  the token's repo grant, so a bare `author:@me` would surface a user's activity
  across every repo they've touched — a token's scope can't prevent it. WAID
  therefore makes scope **mandatory**: the connection names one or more
  `owner/name` `repos`, injected as leading `repo:` qualifiers on the search
  feeds (`prepend_repo_qualifiers`, capped at `MAX_INJECTED_REPOS = 10`) and used
  to filter the REST feeds. An **unscoped feed is rejected** (`require_repos` →
  the `UNSCOPED_ERR` toast), never silently broadened. A `query` that already
  pins scope with a `repo:` / `org:` / `user:` qualifier (`has_scope_qualifier`)
  is an explicit power-user opt-in and is honored verbatim. `normalize_repo`
  tolerates a pasted URL, a `.git` suffix, and stray slashes; these helpers are
  pure and unit-tested.
- **Setup hardening.** `github.rs::api_base` **ignores a `base_url` pointing at
  public github.com** (the field is Enterprise-only), and a notifications fetch
  that 403s returns a clear "use a **classic** PAT with the `notifications`
  scope" error (fine-grained tokens can't reach that endpoint).

### Notion connector (the newest provider)

Notion is a REST connector authed with an **internal integration token**
(`Bearer <token>` + a required `Notion-Version: 2025-09-03` header). The token
only sees databases/pages the user has explicitly **shared** with the integration
(via each page's *Connections* menu); a fetch against an unshared resource
401/403s and degrades to no-signal upstream. One Notion connection plays **two
roles**, selected by the selector's `kind`:

- **`kind: "tasks"` — a database's rows** become panel `IntegrationItem`s, the
  same as any other provider's task feed. Under the 2025-09-03 *data-source*
  model a database contains one or more data sources, so row queries hit
  `/data_sources/{ds_id}/query`; v1 resolves `data_sources[0]` and notes the
  limit. `query` carries the database id or URL (it's **required**, not an
  optional filter).
- **`kind: "page"` — a project page** whose **block text feeds synthesis as
  evidence** (`fetch_page_text`), the same role a `notion.so` link in the body
  plays. It is *not* a task/notification feed: `digest_integrations` and
  `morning_briefing` skip `page` selectors. In the panel a `page` feed shows the
  page as a single row (its title + last-edited time, via `map_page`); there's no
  Max-items cap.

`extract_id` parses a Notion id from a bare id, a dashed id, a page URL, or a
database URL with a `?v=` view. **Notion links in the brief body** are pulled into
synthesis too: `gather_evidence` matches `notion.so` (legacy), `notion.com`, and
`app.notion.com` (current) hosts via `is_notion_app_url` and fetches their page
text. Public `*.notion.site` published pages are intentionally **excluded** (no
token needed → treated as ordinary web links). A `seen_notion_pages` set
de-dupes, so a page referenced both as a body link and as a `page` feed is
fetched once, and Notion page evidence counts toward the synthesis context-budget
split alongside the web-fetched sources.

### Gmail connector (the OAuth provider)

Gmail is the one connector that **doesn't use a pasted token** — it signs in with
**Google OAuth** — yet `provider/gmail.rs` stays as pure as the others: it only
ever receives a ready bearer token and maps the response (`map_message`) into
`IntegrationItem`s. All auth lives in `commands.rs`.

- **Account-scoped grant, not brief-scoped.** A hand-rolled Google OAuth
  **desktop loopback + PKCE** flow (`connect_gmail` — no OAuth SDK, just `reqwest`
  + a `tokio` localhost listener) opens the browser once per Gmail address and
  stores a single grant in the keyring keyed **`gmail.oauth:<account-email>`**,
  shared across every brief whose connection names that `account`. So Gmail
  connections carry **no `bconn:` token**, and `save_brief_connection` skips the
  token write/requirement for them. (Under **WSL** the loopback listener binds
  `0.0.0.0` instead of `127.0.0.1`, because the Windows-host browser's redirect to
  `127.0.0.1` arrives through WSL2's localhost-forwarding relay on `eth0`; the
  `redirect_uri` stays `127.0.0.1`, and PKCE + the unguessable `state` keep the
  briefly-LAN-reachable port safe. Every other platform keeps the tighter
  loopback-only bind.)
- **Bring-your-own client.** WAID ships no Google credentials. The user pastes
  their own Google **Desktop** OAuth client into Settings, stored in the keyring
  as `gmail.client_id` / `gmail.client_secret`. (One-time Google Cloud setup; the
  README spells out the consent-screen "set to **Production**" gotcha that keeps
  the `gmail.readonly` restricted scope's refresh token from being revoked after
  7 days.)
- **One token seam.** `resolve_connection_token(conn, brief_path)` is what the
  generic fetch/test call sites route through: for `Provider::Gmail` it loads the
  account grant and refreshes the access token on demand (`gmail_access_token`,
  with the pure `grant_is_fresh` check); for every other provider it's the
  unchanged `bconn:` lookup.
- **Read-only + metadata-only.** Scope is `gmail.readonly`; fetches use
  `format=metadata` + snippet, so **only subjects and short snippets ever leave
  Gmail — never message bodies**. The selector's `query` is a **required** Gmail
  search string (e.g. `from:acme.com subject:"redesign" newer_than:14d`);
  matching messages become items (subject → title, sender → assignee slot,
  read/unread → status).
- **Feeds, not evidence.** Email feeds (`kind: email`) flow into the panel and the
  digest/morning-briefing, but are deliberately **kept out of `gather_evidence`**
  — synthesis evidence is Notion-pages-only (the sole non-page exception is the
  NeuroSkill Mind State rollup; see its subsection).
- **Query authoring help.** Because most users don't know Gmail's operators, the
  feed form offers a row of one-click **search templates** (recent unread, needs
  my reply, important, starred, has attachment, …; the first is applied as the
  default when a Gmail feed is started) plus an **"describe it" box** wired to
  `generate_gmail_query`, which turns plain English into a single query line via
  the synthesis LLM (a transform of the user's own request, so — unlike the digest
  prompts — no data-not-instructions guard, but the output is pinned to one query
  line by `sanitize_gmail_query`). Both are **display-only**: they fill the field;
  nothing is fetched or written until you save the feed.

### Slack connector (the token-paste search provider)

Slack rides the **unchanged `bconn:` token-paste flow** (unlike Gmail's OAuth):
the user pastes a **user token** (`xoxp-…` with the `search:read` scope) and
*Test connection* validates it; no browser dance. Like every other module
`provider/slack.rs` is **pure / network-only** — the token is loaded from the
keyring by `commands.rs` and passed in.

- **One feed kind: `messages`.** The selector's `query` is a **Slack search
  string** (e.g. `in:#waid from:@dana after:2026-06-01`), run through
  `search.messages` over **whatever the *user* can see** — scoped by the query,
  not by bot membership. Matching messages map (`map_message`) into
  `IntegrationItem`s: the message text → title, sender → assignee slot, the
  channel and a cleaned-up snippet alongside.
- **200-OK error handling.** Like Linear's in-band GraphQL errors, Slack reports
  failures as **HTTP 200 with `{"ok": false, "error": "…"}`**, so
  `slack.rs::check_ok` maps the error slug *after* `read_json`.
- **Feeds, not evidence.** Message feeds (`kind: messages`) flow into the panel,
  the digest, and the morning briefing, but are deliberately **kept out of
  `gather_evidence`** — the same exclusion as Gmail email feeds; synthesis
  evidence is Notion-pages-only (aside from NeuroSkill Mind State).
- **Query authoring help.** As with Gmail, the feed form offers one-click
  **search templates** plus an **"describe it" box** wired to
  `generate_slack_query` (turns plain English into one query line via the
  synthesis LLM, pinned by a sanitizer shared with `generate_gmail_query`).
  Display-only — it just fills the field.
- **One-time Slack setup (per the user).** WAID ships no Slack app: create one
  *From scratch*, add `search:read` under **User** Token Scopes (not Bot — bot
  tokens can't search), install, and copy the **User OAuth Token** (`xoxp-…`).
  Don't enable *Token Rotation* (it converts the token to an expiring
  `xoxe.xoxp-…` that token-paste connections can't refresh). See the README for
  the full walkthrough.

### Figma connector (the file-comments token-paste provider)

Figma rides the **same `bconn:` token-paste flow** as Slack: the user pastes a
**Personal Access Token** (sent in the `X-Figma-Token` header — *not* bearer
auth) and *Test connection* validates it with `GET /v1/me`. Like every other
module `provider/figma.rs` is **pure / network-only** — the token is loaded from
the keyring by `commands.rs` and passed in.

- **One feed kind: `comments`.** The selector's `query` is **required** and
  carries (1) a **Figma file URL or key** — the scope, because the REST API has
  **no cross-file comment search** (a feed is file-scoped, the GitHub
  repo-scoping precedent) — plus (2) an optional **`mentions:me`** / `@me` token.
  `GET /v1/files/{key}/comments?as_md=true` returns the file's comments;
  `map_comment` maps each into an `IntegrationItem`: first line of the message →
  title, author handle → assignee slot, `resolved_at` → `Open`/`Resolved`
  status, `created_at` (already RFC3339) → `updatedAt`.
- **`mentions:me` is heuristic.** There is no structured mention field on a
  GET'd comment, so the filter makes one extra `GET /v1/me` to learn the handle
  and keeps comments whose message contains that handle as a bounded token
  (best-effort: can miss renamed/group mentions, can catch the name typed in
  prose). On a `/v1/me` failure it **degrades to all comments** rather than
  erroring. The default (no `mentions:me`) surfaces recent **unresolved**
  comments — a deterministic, always-correct signal; `mentions:me` keeps matches
  regardless of resolved state (so you still see a resolved thread you were
  tagged in).
- **No per-comment deep link.** The API returns no comment-pin anchor, so the
  item `url` lands the user in the file (`figma.com/design/{key}/`).
- **Feeds, not evidence.** Comment feeds (`kind: comments`) flow into the panel,
  the digest, and the morning briefing, but are **kept out of `gather_evidence`**
  automatically — that path is Notion-pages-only (plus NeuroSkill Mind State),
  same as Slack/Gmail.
- **No query-authoring help.** Unlike Gmail/Slack there's no search grammar to
  author, so there's deliberately no `generate_figma_query` / templates — the
  query is just a file URL plus the optional `mentions:me` flag.
- **One-time Figma setup (per the user).** WAID ships no Figma app: generate a
  Personal Access Token under **Settings → Account → Personal access tokens**
  with the **`file_comments:read`** (and `current_user:read`) scope, then paste
  it.

### NeuroSkill connector (the local-EEG `## Mind State` region)

NeuroSkill is the odd connector out: its output is a **deterministic, regenerable
body region** (`## Mind State`, marker `waid:mind`, a sibling of `## Activity`),
**not** panel items — so it doesn't dispatch through `provider/` (documented
pure/network-only). It lives in its own `src-tauri/src/neuroskill/` module, which
reads **local SQLite read-only** and performs exactly one write, NeuroSkill's
`label` command over the daemon's local HTTP API. It never links/vendors any
NeuroSkill/GPL code — the integration is strictly at the process/file boundary
(NeuroSkill is GPL-3.0; WAID is MIT).

- **Two features, one join key.** (A) `## Mind State` shows a rolling-window
  (default **14d**; `today|7d|14d|30d`) aggregate of EEG focus/engagement/mood/
  relaxation over the brief's **labeled** sessions. (B) **Labeled-session launch**:
  WAID writes `waid:brief=<slug>:start`/`:end` labels into NeuroSkill at
  launch/close so attribution is **authored, never guessed** from window/terminal
  data. The shared join key is the brief slug (file stem; overridable via the
  selector's `slug:` token). Labels are matched on the **exact**
  `waid:brief=<slug>:` prefix, so a sibling brief's sessions never attribute here;
  `eeg_timeseries` itself has **no project column** — an epoch belongs to a brief
  solely by timestamp containment in one of its paired intervals. Corollary: a
  forgotten `:end` attributes up to the 4h cap (or `now`) to that brief.
- **Token from a file, not the keyring.** The daemon gates its API with a bearer
  token it writes to `<config>/skill/daemon/auth.token` (`$XDG_CONFIG_HOME`/`~/.config`,
  `%APPDATA%` on Windows, `~/Library/Application Support` on macOS). WAID reads that
  file at call time (`neuroskill::load_token`) and sends `Authorization: Bearer` on
  the write — it never stores the token. So `save_brief_connection` /
  `resolve_connection_token` still route NeuroSkill through the same **keyring**-optional
  path as Gmail (no `bconn:` secret). The connection carries optional `wsUrl`
  (daemon URL), `dataDir`, and `tokenPath` overrides for the WSL2↔Windows-host split
  (on WSL2 the daemon writes its token on the Windows side, so `tokenPath` points at
  the `/mnt/c/.../AppData/Roaming/skill/daemon/auth.token`).
- **Pipeline (mirrors `## Activity`, no LLM).** `sync_mind_state(path)`: read
  `labels` → pair `:start`/`:end` into intervals (unclosed start capped at 4h or
  `now`) →
  rolling-window filter → read `eeg_timeseries` epochs per interval → deterministic
  rollup (`aggregate.rs`, the fixture-tested core: means, trend = first-half vs
  second-half, peak-focus block) → render numbers → `upsert_marked_block(waid:mind)`.
  Frontmatter and every sibling region preserved byte-for-byte; a read/parse
  failure is a toast + no-op, never a partial write.
- **Read scope is hard-bounded.** The only `SELECT`s ever issued are the two query
  constants against **`eeg_timeseries`** and **`labels`** — enforced by a read-scope
  guard test that rejects every other table (so the user's `track_*` toggles are
  harmless and no window/file/terminal text can leak into a portable brief). DBs
  are opened `mode=ro&immutable=1` (a static snapshot; no write-lock contention
  with the always-on daemon).
- **Portable output only.** The region renders only WAID-computed numbers and
  formatted times — raw `labels.text` is never echoed (injection surface; WAID
  controls only the `waid:brief=` format).
- **The write side (`mark_brief_session`).** Fires `waid:brief=<slug>:(start|end)`
  as an authenticated `POST /` to the daemon's local HTTP API (`client.rs`: a
  `reqwest` POST of `{"command":"label","text":…}` + `Authorization: Bearer`,
  default origin `http://127.0.0.1:18444`). It handles a `401` (missing/wrong token)
  and an in-band `{"ok":false}` as friendly errors. The frontend fires `:start` on
  the first launch/open action and via an explicit **Start/End session** button;
  `:end` fires from that button and, as safety nets, on navigating away
  (`onDestroy`), with the 4h interval cap as the final backstop. The store surfaces
  a failed `:start` as a toast (and clears the optimistic "active" state); the
  automatic `onDestroy` `:end` stays silent. Best-effort: an unreachable daemon
  never blocks the launch; a brief with no NeuroSkill connection is a silent no-op.
- **A body region, not panel items.** `mind` selectors are excluded from
  `digest_integrations` / `morning_briefing` (alongside Notion `page`). The panel
  renders them as a slim status row with a refresh that calls `sync_mind_state`,
  not item cards.
- **Feeds the Current State synthesis (Phase 2).** When an LLM provider is
  configured, `gather_evidence` adds the MindState rollup as a small **descriptive**
  evidence chunk (`neuroskill::evidence_text` — numbers + worded trends only, never
  raw labels, flagged "context, not a directive"), but **only for briefs with a
  NeuroSkill `mind` feed**. It's the lone non-Notion-page exception to the
  evidence pool; a read failure or empty window is skipped, never fatal, and it
  doesn't consume the web-source budget split.

### Optional LLM layer (reuses the synthesis provider)

On top of the local rollup, with a provider configured (`make_provider`):

- **`digest_integrations(path)`** — re-fetches one brief's task / notification /
  **email** / **Slack-message** / **Figma-comment** items and asks the model for
  a short prose digest. Display-only; can
  be **snapshotted into `## Captures`** via `append_capture`. (Notion `page` feeds
  are excluded — they belong to synthesis, not the digest.)
- **`morning_briefing()`** — re-fetches every brief's items, groups by project,
  and asks for a tight cross-brief briefing. (Notion `page` feeds excluded here
  too.)
- Same guardrail as synthesis: **every fetched item is data, never
  instructions** (item titles/fields, Notion page text, Gmail subjects /
  snippets, Slack message text, and Figma comment text can all carry injected
  text); output is plain
  prose that can't trigger any action or write anywhere.

### Caching (frontend)

`stores/integrations.svelte.ts` keeps an in-memory cache keyed by
`path + connection + kind + query` (and per-brief LLM digests). No disk
persistence, no auto-polling: fetches are lazy on brief open and on manual
refresh, and the last good result is kept so switching briefs back doesn't
refetch (and a failed refresh degrades to "stale + error" rather than going
blank).

### UI

- **`IntegrationPanel.svelte`** — renders each selector's items (status chip,
  priority/project/assignee/relative-time), the local summary line, a refresh
  button, and (when a provider is configured) the AI-digest card with
  Generate / Regenerate / Snapshot. Feed rows are keyed by
  `connection + kind + query` so multiple feeds on one connection render
  independently; the kind icon/label adapt per provider (`kindIcon` / `kindLabel`
  take a `provider`).
- **`IntegrationsModal.svelte`** — manage a brief's connections + selectors
  (add/edit/delete, test connection). The credential step puts the **token field
  first** (above the optional base URL) and pairs every credential field with a
  **`CredentialHelp` help icon** (per-provider setup steps + scopes + a docs
  link, data from `credentialHelp.ts`). The add-feed form adapts to Notion: the
  segmented "Database / table" vs "Project page" choice, a **required** URL input
  (not an optional filter), no Max-items cap for a page feed, and helper text
  reminding you to share the resource with the integration. For **Gmail** it drops
  the API-token field for a **"Connect Google account"** OAuth button (no footer
  on that step — the button runs `connect_gmail` and saves the connection with the
  discovered account email and no token), and the feed step asks for a **required
  Gmail search query** with operator hints, **one-click templates**, and an
  **AI "describe it"** input (`generate_gmail_query`) that fills the field. For
  **Slack** it's an ordinary token-paste connection (`xoxp-…`), and the feed step
  asks for a **required Slack search query** with the same templates + AI
  "describe it" input (`generate_slack_query`). The
  **manage view** also supports inline edits: **rename a connection's label**
  (re-saved with an empty token, so only the label changes) and **edit a feed's
  query in place** — since feed identity is `(connection, kind, query)`, an edited
  query drops the old selector and saves a new one, then re-fetches the panel.
- **Unified titlebar (`Titlebar.svelte` + `AppMenu.svelte`)** — the app draws its
  own window chrome across the full width (breadcrumb of the selected brief,
  *Sync all*, the morning-briefing trigger, and the View & appearance menu),
  replacing the stock OS title bar and the sidebar's old brand header. Window
  controls flip by platform (sniffed from the webview `userAgent`): **macOS**
  keeps its native traffic lights via the **Overlay** title-bar style
  (`titleBarStyle: "Overlay"` + `hiddenTitle` in `tauri.conf.json`); **Windows
  and Linux/WSL** turn OS decorations off (`set_decorations(false)` in `lib.rs`,
  guarded `#[cfg(not(target_os = "macos"))]`) and draw their own
  minimize/maximize/close caption buttons (the extra `core:window:*` capabilities
  are allow-listed in `capabilities/default.json`).
- **Custom window shell + resize grips (`ResizeHandles.svelte`)** — the native
  window is **borderless and transparent** (`transparent: true` +
  `macOSPrivateApi` in `tauri.conf.json`, `macos-private-api` feature on the
  `tauri` crate; `html, body { background: transparent }`), so the whole app
  lives inside a rounded, bordered **app panel** drawn in `+page.svelte` with a
  14px transparent gutter around it (desktop showing through, room for the
  panel's `--shadow-win`). With OS decorations off there is no native frame to
  grab, so `ResizeHandles.svelte` lays invisible grips along the window edges and
  hands the drag to the OS via `startResizeDragging` (and `setCursorIcon` on
  hover, since WebKitGTK ignores the CSS `cursor` on transparent overlays —
  both gated by the new `core:window:allow-start-resize-dragging` /
  `allow-set-cursor-icon` capabilities). Each grip straddles the transparent
  gutter **and** reaches a few px onto the opaque panel, because pointer events
  over a fully transparent gutter pass straight through on Linux/WSL. macOS keeps
  its native frame, so the grips are skipped there (same `userAgent` sniff).
- **View & appearance menu (`AppMenu.svelte`)** — anchored to the titlebar's
  `tune` button (moved out of the sidebar's old brand header). Besides the
  appearance controls (light/dark, accent, sidebar list style, density, and the
  **Brief layout** segmented control) and the GitHub token / Anthropic key /
  LLM-provider config, it manages the **bring-your-own Google OAuth client**
  (`gmail.client_id` / `gmail.client_secret` in the keyring): save / clear, gated
  so Connect only works once a client is stored. Token fields here carry the same
  **`CredentialHelp`** icons as the integrations modal.
- **`BriefingModal.svelte`** — the cross-brief "Morning briefing" view.
- **`ProviderTile.svelte`** — a provider monogram tile in the provider's brand
  color (`.ptile-<provider>` classes in `app.css`, so dark-mode tweaks stay in
  CSS). All provider **display metadata** — label, monogram, query placeholder,
  which kinds each provider exposes (`tasks` / `notifications` / `pulls` /
  `commits` / `page` / `email` / `messages`), and which form fields it needs
  (`needsBaseUrl` / `needsAccount`) —
  lives in the frontend-only **`providers.ts`** `PROVIDERS` registry +
  `PROVIDER_ORDER`, along with the `Kind` type and the provider-aware `kindIcon` /
  `kindLabel` helpers (for Notion, `tasks` → "Database / table", `page` →
  "Project page"; for Gmail, `email` → "Email" with a `mail` glyph; for Slack,
  `messages` → "Messages" with a `chat` glyph; for GitHub, `pulls` → "Pull
  requests" / `merge` glyph and `commits` → "Commits" / `commit` glyph). The Rust
  side still owns the actual fetch/validate; this is purely display + form shape.
  (GitHub exposes `tasks` / `notifications` / `pulls` / `commits`; Notion exposes
  `tasks` + `page`; Gmail exposes `email`; Slack exposes `messages`; Jira needs
  base URL + account, Asana needs an account/workspace id, GitHub needs a base
  URL.) GitHub's required repo scoping and setup hardening (Enterprise-only
  `base_url`, the classic-PAT notifications error) are covered in §5's GitHub
  connector subsection.

---

## 6. Current Visual Identity

The visual layer has a named theme and shares branding with What's Next.

- **Theme: "Tidewater · Refined."** A CSS-variable token system in `app.css`
  drives light/dark, accent, and density from one place — a Mercury-influenced
  restraint pass over the original Tidewater tokens: **1px hairline dividers
  instead of boxed/colored cards, a more neutral palette, tighter/smaller type,
  and the accent used sparingly as a true accent (links, primary buttons,
  selection bar) rather than as a fill.** `--accent` (default `#1e88e5`, the
  "What's Next blue" seed) is **overridable per-tree** on the app shell; derived
  tokens (`--hover`, `--accent-tint`, `--accent-line`, `--border-soft`, …) resolve
  it lazily via `color-mix`, so changing the accent recolors everything.
- **Fonts:** **Inter** (body), **Nunito** (brand/headings), and **Material
  Symbols Rounded** (icons, via `Icon.svelte` + the `.msym` class).
- **Shared brand with What's Next.** A fixed green **"W" brand mark**
  (`BrandMark.svelte`, `--wn-brand-green: #40a87e`, a single `logo.svg` path via
  CSS mask so it can be recolored per surface) plus a **"rock priority" status
  motif** — WAID shares What's Next's branding system.
- **Status palette (shared light + dark), muted in the refresh:** active
  `#3f9d63`, paused `#d3982f`, blocked `#d65a55`, archived `#9aa0aa`. Each element
  sets `--sc` inline; `.pill` / `.sdot` derive tinted fills from it (`status.ts`
  keeps the same API — only the hex values changed — and stays the single source
  of truth for order/color/label).
- **Provider brand tiles.** Each PM provider gets a monogram tile in its own
  brand color via `.ptile-<provider>` classes in `app.css` (so dark-mode tweaks
  stay in CSS); `ProviderTile.svelte` + `providers.ts` drive label/monogram/blurb
  (Linear "L", Jira "J", Asana "A", GitHub "G", Notion "N", Gmail "@" — "G" is
  taken, and "@" reads as email — and Slack "S").
- **Density toggle.** Comfortable default; a `.dense` class on the shell tightens
  the detail pane (the `--pane-px` / `--pane-py` / `--title-size` / `--md-size`
  tokens).
- **Brief layout (new setting).** Where a brief's live-state panel sits is
  user-selectable from the titlebar's View & appearance menu — `briefLayout` in
  `settings.svelte.ts` (`two-col` | `body` | `quiet`, default `two-col`):
  a right-hand 320px `--rail-bg` rail, below the brief body under a hairline
  divider, or a slim feed-strip across the top. Persisted to `localStorage`
  alongside accent / sidebar style / density.
- **Layout:** a **unified titlebar** (the app's own window chrome — breadcrumb,
  *Sync all*, briefing, View & appearance menu) over a still **two-pane** body —
  a sidebar (search/filter, project name, status
  pill, last-opened) and a detail pane (rendered brief body + link/webhook
  buttons + sync/synthesis/bootstrap affordances + the integration panel, the
  last placed per the brief-layout setting above). The empty-integrations state is
  a slim provider-glyph **row** ("Connect →"), or a compact stacked card in the
  two-column rail.
- **Dependency-light, still true.** No Tailwind typography plugin; rendered-
  markdown styling is hand-rolled in `app.css` under `.markdown` (including
  wikilink + dangling-link styles). Tweaking that block restyles every brief.

---

## 7. Features (v1)

- **Two-pane layout** — sidebar of projects + detail pane.
- **Markdown rendering** of the brief body (with Obsidian `[[wikilinks]]` +
  backlinks when in a vault).
- **Edit mode** — toggle to a raw textarea and save back to the `.md`
  (round-trips the whole file; frontmatter preserved). `⌘/Ctrl+S` saves.
- **Brief bootstrap** — a stub brief offers a **"Generate the initial brief"**
  CTA with four methods (point at a folder/repo, a GitHub URL, a 3-question
  guided interview, or copy/paste a handoff prompt). Each produces a *proposed
  raw file* that opens in **edit mode** for review — nothing is written until you
  Save. Metadata is deterministic; an AI-drafted body is added when a provider is
  set. Bootstrap seeds Current State / Open Questions through the synthesis
  markers so a later Refresh regenerates them in place. *(See §2.)*
- **Link buttons** — open URLs in the default browser; **Open in Obsidian** deep
  link when applicable.
- **Webhook buttons** — fire GET/POST/PUT/PATCH/DELETE from the brief's launch
  row with a success/failure toast. Webhooks carry **custom headers**; one header
  value may reference a keyring-backed secret via a `{{secret}}` sentinel
  (resolved server-side at fire time). Header *shapes* round-trip in frontmatter
  (edited in Edit mode); the secret lives in the OS keyring keyed
  `whook:<brief-path>:<id>`. Firing only makes an HTTP request — never a write
  into the `.md`. *(The "Tidewater · Refined" refresh removed the standalone
  header **Manage webhooks** button for restraint; the `save_brief_webhook` /
  `delete_brief_webhook` commands and `WebhooksModal` remain in the code but are
  not currently surfaced in the UI.)*
- **Quick capture** — `⌘/Ctrl+K` (or global `Ctrl+Shift+Space`) → modal → pick a
  project → append a timestamped note under its `## Captures` heading.
- **Search & status filtering** — text filter (`⌘/Ctrl+F`) + status filter;
  archived briefs hidden by default, surfaced via the "archived" status (archive
  view).
- **Brief sync (deterministic)** — pull live state (open PRs/issues, last push,
  CI via the combined-status API, latest release) from a brief's GitHub link or
  explicit `sources` into the managed `## Activity` block. Frontmatter and prose
  are never touched.
- **PM integrations (deterministic)** — per-brief Linear / Jira / Asana / GitHub
  / Notion / Gmail / Slack connections + selectors; the detail-pane panel shows
  normalized tasks / notifications / Notion-database rows / recent Gmail matches /
  matching Slack messages and a local
  rollup. One connection can host several feeds. Tokens live in the OS keyring;
  Gmail instead signs in via **Google OAuth** (read-only, metadata-only) with an
  account-scoped grant in the keyring. *(See §5.)*
- **AI synthesis (optional)** — with a provider configured (local **Ollama** or
  **Anthropic**), Refresh also writes a `## Current State` summary and an
  `## Open Questions` inner block from the brief's links + Notion page text +
  `## Captures`. The model writes **only** those two app-owned regions and returns
  a closed `{ current_state, open_questions }` schema; all fetched content is
  treated as **data, never instructions**. Manual trigger only.
- **AI integration digest + morning briefing (optional)** — a per-brief prose
  digest of live PM items (snapshot-able into Captures) and a cross-brief
  "morning briefing" across all briefs. Display-only; same data-not-instructions
  guard. *(See §5.)*
- **Secret storage in the OS keyring** — a GitHub token (private-repo sync), an
  Anthropic API key (synthesis/digests), per-brief PM connection tokens,
  per-brief webhook secrets, and Gmail's OAuth client + account grants live in the
  platform keychain, never in settings or env. Keys: `github.token`,
  `anthropic.api_key`, `bconn:<brief-path>:<id>`, `whook:<brief-path>:<id>`,
  `gmail.client_id` / `gmail.client_secret` (bring-your-own Google Desktop
  client), and `gmail.oauth:<account-email>` (the account-scoped grant, shared
  across briefs — not `bconn:`-keyed) — service `com.jelanijohn.waid`.
  (**NeuroSkill keeps no keyring entry** — the daemon's bearer token is read at
  call time from its own on-disk `auth.token` and never stored by WAID, so like
  Gmail it has no `bconn:` token.)
- **Appearance** — light/dark, accent, sidebar list style (Rows / Compact /
  Rocks), density, and the **Brief layout** control (two-column rail / body-first
  / quiet-top), all from the **View & appearance** menu in the titlebar.

### Keyboard / shortcuts

- **Global** `Ctrl+Shift+Space` — system-wide hotkey that surfaces the window and
  triggers quick capture via a `waid://quick-capture` event (desktop-only; may be
  intercepted by some Linux WMs — a known limitation, not a bug).
- **In-app** `⌘/Ctrl+K` — quick-capture modal.
- **In-app** `⌘/Ctrl+F` — focus the sidebar search/filter.
- **In-app** `⌘/Ctrl+S` — save in edit mode.

---

## 8. Out of Scope for v1 (Planned)

Several earlier "planned" items have now **shipped** — Obsidian vault storage,
encrypted/keyring secret storage, search + status filters + archive view,
sanitized seed briefs, the dedicated Linear / Asana / Jira / **Notion** /
**Gmail** / **Slack**
connectors (plus a richer GitHub connector — Gmail via Google OAuth), **and brief
bootstrap** (folder / GitHub / interview / paste). Remaining planned:

- **Drag-to-reorder** the project list.
- A **richer markdown editor** (CodeMirror / Tiptap / Milkdown).
- A dedicated borderless **"spotlight" window** for quick capture (today the
  global shortcut focuses the main window).
- **In-process inference** for synthesis — "local" means Ollama over localhost
  HTTP, not embedded llama.cpp / Candle / mistral.rs.
- A **diff-and-confirm preview gate** remains unneeded — synthesis and digests
  only write regenerable, app-owned regions (and digests don't write at all
  until snapshotted); bootstrap only proposes a draft into edit mode.
- **Due-date / overdue signal** in the integration rollup (`overdue` is `null`
  today).

**Explicitly never planned:** mobile/web versions, auth, multi-user, sync.

---

## 9. Working Principles & Decisions

Durable choices worth preserving across conversations:

1. **Local-first, for real.** No server, no cloud DB, no auth — just a desktop
   app reading markdown files from a folder on disk. (LLM features are opt-in;
   the local-Ollama path keeps everything on-device.)
2. **Portable forever.** One plain `.md` per project; if WAID disappears, the
   briefs still open in any editor. Managed regions are HTML-comment markers, so
   they render to nothing.
3. **Obsidian-compatible by design.** Standard frontmatter + standard markdown
   only, no proprietary format.
4. **Dependency-light.** Prefer hand-rolled solutions over heavy plugins — the
   markdown CSS instead of a typography plugin; no in-process ML runtime; PM
   connectors are plain JSON requests, not provider SDKs or a GraphQL client.
5. **Edits round-trip the raw file.** Frontmatter is never reordered or lost.
   Writes either re-attach the verbatim `prefix` from `split_for_body_edit`
   (body-marker writes) or splice a single named key via a `serde_yaml::Mapping`
   while preserving the rest (`touch_brief`, the connection/selector commands).
6. **Bounded blast radius for the agents.** Synthesis writes only app-owned,
   regenerable regions; integration fetches/digests never write to the `.md`
   (digests only when the user snapshots them); all fetched content — web pages,
   GitHub data, PM item titles/fields, Notion page text, bootstrap evidence,
   Captures — is **data, never instructions**; output schemas can't touch
   status/links/tags/webhooks; any error leaves the file untouched. So no
   diff-and-confirm gate is needed in v1.
7. **Per-brief PM connections.** Connection *metadata* lives on the brief
   (portable, Obsidian-safe); the *token* lives in the keyring scoped by brief
   path + id. Selectors reference a brief's own connections by id, and feed
   identity is `(connection, kind, query)` so one connection can host several
   feeds. **Gmail is the deliberate exception:** it authenticates with Google
   OAuth, and its grant is **account-scoped** (`gmail.oauth:<email>`), shared
   across every brief pointing at that account — so the `provider/gmail.rs` module
   stays pure (takes a bearer token), but the connection has no `bconn:` token and
   `resolve_connection_token` is the seam that hides the difference from callers.
8. **Bootstrap seeds, synthesis owns.** Initial-brief generation writes Current
   State / Open Questions *through the same `waid:state` / `waid:questions`
   markers the synthesis agent owns*, so the first Refresh regenerates them in
   place instead of duplicating. Bootstrap never writes to disk — it proposes a
   raw file into edit mode; the human's Save is the commit gate (same gate as
   every other write path).
9. **Single-user, desktop-only.** Don't propose features that assume cloud,
   accounts, or collaboration.

### Backend conventions

New backend functionality = a `#[tauri::command]` in `commands.rs` + an entry in
the `generate_handler!` list in `lib.rs` + a typed wrapper in `lib/tauri.ts`
(the **only** place that names backend commands). A new PM provider = a submodule
under `src-tauri/src/provider/` with a pure, fixture-tested `map_*` mapper, wired
into the `fetch` / `validate` dispatch in `mod.rs` (and a `PROVIDERS` entry in
`providers.ts` for display). Frontend uses 2-space indent, double quotes, Svelte 5
runes (`$state`/`$derived`/`$props`); reactive stores use the `.svelte.ts`
extension. Keep `types.ts` in sync with the Rust structs.

---

## 10. Workflow Patterns

- **Claude (web/desktop)** — planning, design decisions, code generation.
- **Claude Code** — file-level implementation and CLI work (Svelte components,
  Rust commands, `pnpm`/`cargo`). `CLAUDE.md` in the repo root carries the
  working guidance (now including the `provider/` architecture). Handoff specs
  for larger changes land as root `.md` docs and are cleared once applied.
- **Claude Design** — well-suited to mocking up the dashboard UI (sidebar,
  detail pane, quick-capture modal, integration panel/modals, bootstrap modal)
  because its HTML/web output maps cleanly onto this Svelte + Tailwind frontend;
  hand off the locked direction to Claude Code.
- **Filesystem MCP** — used to read the repo directly at `…/Projects/waid`.

---

*Compressed snapshot — for full detail, see the repo (`README.md`, `CLAUDE.md`,
`src-tauri/src/commands.rs`, `src-tauri/src/provider/`, `src/lib/`).*
