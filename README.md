# WAID — What Am I Doing?

A local-first desktop dashboard for managing your active projects. WAID is the
**macro** companion to *What's Next?* (which answers "what task should I work on
right now?"). WAID answers: **"what's the state of all my projects?"**

It lists your projects, renders each project's context **brief**, and lets you
launch into work — open a Claude project, a repo, fire a webhook, or jot a quick
note. Personal tool, not a team tool.

## Why it's built this way

- **Local-first, for real.** No server, no cloud DB, no auth. Just a desktop app
  reading markdown files from a folder on disk.
- **Portable forever.** Each project is one plain `.md` file with YAML
  frontmatter + a markdown body. If WAID disappears, your briefs still open in
  any editor.
- **Obsidian-native.** Standard frontmatter + standard markdown only — no
  proprietary format. Point WAID's briefs folder at an Obsidian vault (or any
  subfolder of one) and it scans recursively, renders `[[wikilinks]]`, shows
  backlinks, and can jump you straight into Obsidian.

## Stack

- [Tauri 2](https://v2.tauri.app/) — desktop shell + Rust backend
- [SvelteKit](https://svelte.dev/) (Svelte 5, TypeScript) — frontend (SPA mode)
- [Tailwind CSS v4](https://tailwindcss.com/) — styling
- `marked` + `DOMPurify` — markdown rendering
- Plain `.md` files on disk — data

## Prerequisites

- **Node** 18+ and **pnpm** (`corepack enable pnpm`)
- **Rust** (stable) via [rustup](https://rustup.rs)
- Platform build deps for Tauri — see
  [tauri.app/start/prerequisites](https://v2.tauri.app/start/prerequisites/).
  On Debian/Ubuntu:

  ```bash
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
  ```

## Run

```bash
pnpm install
pnpm tauri dev      # launches the desktop app with hot reload
```

## Build

```bash
pnpm tauri build    # produces a native bundle in src-tauri/target/release/bundle
```

Other useful scripts:

```bash
pnpm check          # type-check the frontend (svelte-check)
cargo test          # run backend unit tests (from src-tauri/)
```

## Data: the briefs folder

Each project is a single markdown file. WAID reads every `.md` in the configured
**briefs directory**.

- **Default location:** `~/WAID/briefs`. On first run, if the folder is empty,
  WAID seeds it with sample briefs so you have real content immediately.
- **Change it:** use the **Change** link at the bottom of the sidebar to pick a
  different folder (e.g. an Obsidian vault). The choice is saved to the app's
  config dir.
- This repo also keeps a `briefs/` folder with the same sample briefs, used for
  development and as the bundled seed content.

### Using an Obsidian vault

Point the briefs folder at your vault (or any subfolder of it) via **Change**.
WAID then:

- **Scans recursively** — every `.md` under the folder becomes a brief, while
  Obsidian's `.obsidian/` (and other dot-folders like `.trash/`, `.git/`) are
  skipped.
- **Renders `[[wikilinks]]`** in the brief body — links to other briefs are
  clickable (they select that brief); dangling links are shown muted. Aliases
  (`[[note|label]]`) and heading anchors (`[[note#section]]`) are supported.
- **Shows backlinks** — a "Linked from" list of every other brief that
  wikilinks to the current one.
- **Opens in Obsidian** — when the folder is inside a vault, an *Open in
  Obsidian* button deep-links the brief via `obsidian://open` so you can edit it
  with Obsidian's full editor.

#### "Open in Obsidian" — one-time setup

The button uses an `obsidian://open?vault=<name>&file=<path>` deep link, which
only works if **Obsidian already knows the vault**. Two requirements:

1. **The vault has been opened in Obsidian at least once** (so it's registered).
   If you point WAID at a vault you actively use in Obsidian, this is already
   true and the button just works. If you've never opened the folder in
   Obsidian, do it once — *Open folder as vault* — or you'll get a "Vault not
   found" popup.
2. **The vault's name matches its folder name.** WAID sends the vault root's
   folder name (the folder containing `.obsidian/`). If you renamed the vault
   inside Obsidian to something else, the link won't resolve.

WAID can't auto-register the vault for you — there's no reliable cross-platform
way to do that from outside Obsidian, so this stays a one-time manual step.

> **WSL note:** running WAID as a Linux build under WSL while using Obsidian on
> Windows works, but the vault must live on the **Windows** filesystem (e.g.
> `C:\Users\you\Vault`, which WAID reads via `/mnt/c/...`). Windows Obsidian
> can't watch a vault stored on the WSL side (`\\wsl.localhost\...`) — it fails
> to load with an `EISDIR` error. You'll also need a URL handler such as
> `wslview` (from the `wslu` package) so WAID can hand `obsidian://` links to
> Windows.

### Brief format

```markdown
---
name: Sample Web App
status: active            # active | paused | blocked | archived (free-form; rendered as a pill)
description: Example project brief
tags: [sample, web, example]
links:
  - label: Claude Project
    url: https://claude.ai/project/xxx
  - label: GitHub
    url: https://github.com/...
webhooks:
  - label: Deploy staging
    url: https://...
    method: POST          # defaults to POST if omitted
    body: '{"env":"staging"}'   # optional; sent as JSON
last_opened: 2026-05-31T10:00:00Z
---

# Project context

The full markdown brief lives here.
```

## Features (v1)

- **Two-pane layout** — sidebar of projects (name, status pill, last-opened) and
  a detail pane.
- **Markdown rendering** of the brief body.
- **Edit mode** — toggle to a raw textarea and save back to the `.md` file
  (round-trips the whole file, so your frontmatter is never mangled). ⌘/Ctrl+S
  saves.
- **Link buttons** open URLs in your default browser.
- **Webhook buttons** fire GET/POST (and PUT/PATCH/DELETE) requests with a toast
  on success/failure.
- **Quick capture** — press **⌘/Ctrl+K** (or the global **Ctrl+Shift+Space**) to
  pop a modal, pick a project, and append a timestamped note under its
  `## Captures` heading.
- **Search & status filtering** — filter the sidebar by text (**⌘/Ctrl+F**) and
  by project status. Archived briefs are hidden by default; picking the
  "archived" status surfaces them (the archive view).
- **Light/dark mode** toggle.
- **Brief sync** — pull live state (open PRs/issues, last push, CI, latest
  release) from a brief's GitHub link or explicit `sources` into a managed
  `## Activity` block. Deterministic; frontmatter and prose are never touched.
- **AI synthesis** _(optional)_ — when an LLM provider is configured (local
  **Ollama** or **Anthropic**), Refresh also synthesizes a `## Current State`
  summary and an `## Open Questions` list from the brief's links and your
  `## Captures` notes. The model only ever writes those two app-owned regions —
  never status, links, tags, webhooks, frontmatter, or Captures — and all
  fetched content is treated as data, never instructions. See
  [AI synthesis](#ai-synthesis-optional) below.
- **Secret storage in the OS keyring** — tokens/keys for authenticated
  integrations (a GitHub token for private-repo sync, an Anthropic API key for
  synthesis) live in the platform keychain, never in settings or env.

### AI synthesis (optional)

Synthesis turns the deterministic fetchers into *evidence* a model reasons over,
then writes a short summary back into two app-owned regions of the brief:

| Region | Markers | Written by |
|---|---|---|
| `## Activity` | `waid:sync:start/end` | Brief sync (deterministic — no LLM) |
| `## Current State` | `waid:state:start/end` | Synthesis (regenerated wholesale) |
| `## Open Questions` → inner block | `waid:questions:start/end` (nested) | Synthesis (inner block only) |

The markers are HTML comments, so they render to nothing and the files stay
portable / Obsidian-native. Everything outside the markers — your prose, your
`## Captures`, all frontmatter — is preserved byte-for-byte.

**Configure it** in the settings popover (the ⚙/tune button) under *AI
synthesis*:

- **Ollama (local)** — runs against a local [Ollama](https://ollama.com) server.
  Set the base URL (default `http://localhost:11434`) and pick a pulled model.
  Nothing leaves your machine.
- **Anthropic (cloud)** — set a model and save an API key (stored in the OS
  keyring). Sends evidence to the Anthropic API.

Then hit **Refresh** on a brief (or *Sync all* in the sidebar). Synthesis is
manual — there's no auto-sync on open or timer.

**Open Questions — known tradeoff:** the model regenerates only the *inner*
`waid:questions` block, wholesale, each run (so it can retract resolved
questions). Your own questions live *above* the block and survive untouched, but
anything you type *inside* the block is overwritten on the next synthesis —
answer questions or add your own above it.

**Safety:** all fetched content (web pages, source JSON, GitHub data, your
Captures) is treated as **data, never instructions**. The model's output schema
is closed to two fields, so it structurally cannot change status, links, tags,
or webhooks, or fire anything. Web fetches are GET-only and truncated. Any error
(fetch, provider, or unparseable output) leaves the `.md` untouched.

## Project structure

```
waid/
├── src/                      # SvelteKit frontend
│   ├── lib/
│   │   ├── components/       # Sidebar, ProjectDetail, MarkdownView, QuickCapture, Toasts…
│   │   ├── stores/           # projects, theme, toasts
│   │   ├── tauri.ts          # wrappers around invoke / plugins
│   │   ├── types.ts          # Brief / Link / Webhook types
│   │   ├── markdown.ts       # marked + DOMPurify
│   │   └── time.ts           # relative-time helper
│   └── routes/               # +layout, +page (main view)
├── src-tauri/                # Rust backend
│   └── src/
│       ├── lib.rs            # plugin + command registration, global shortcut
│       └── commands.rs       # briefs · webhooks · sync · LLM synthesis · keyring · settings
├── briefs/                   # sample briefs (dev + bundled seed)
└── README.md
```

## Out of scope for v1 (planned)

- **Drag-to-reorder** the project list.
- A richer markdown editor (CodeMirror / Tiptap / Milkdown).
- A dedicated borderless "spotlight" window for quick capture.
- **In-process inference** for synthesis — local means Ollama over localhost
  HTTP, not embedded llama.cpp / Candle / mistral.rs.
- Dedicated Linear / Asana connectors (they'll ride the existing `sources` +
  keyring path), and a diff-and-confirm preview gate (unneeded — synthesis only
  writes regenerable, app-owned regions).

Mobile/web versions and any auth/multi-user/sync are explicitly **not** planned —
WAID is single-user, local, desktop-only by design.
