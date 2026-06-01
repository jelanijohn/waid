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
- **Obsidian-compatible by design.** Standard frontmatter + standard markdown
  only — no proprietary format. (Backing WAID with an Obsidian vault is a
  planned follow-up; see the TODO in `src-tauri/src/commands.rs`.)

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

### Brief format

```markdown
---
name: Gluefi
status: active            # active | paused | blocked | archived (free-form; rendered as a pill)
description: Fintech infrastructure for tokenized finance, Japan focus
tags: [fintech, web3, japan]
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
- **Light/dark mode** toggle.

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
│       └── commands.rs       # list/read/save/touch/capture/webhook + settings
├── briefs/                   # sample briefs (dev + bundled seed)
└── README.md
```

## Out of scope for v1 (planned)

- **Obsidian vault as storage backend** (TODO in `commands.rs`).
- **Encrypted secret storage** for webhook auth tokens (OS keyring).
- **Search across briefs**, status filters, drag-to-reorder, archive view.
- A richer markdown editor (CodeMirror / Tiptap / Milkdown).
- A dedicated borderless "spotlight" window for quick capture.
- **Sanitize the bundled seed briefs** in `briefs/` into generic samples. They
  currently hold real project data (these get baked into the binary via
  `include_str!` in `commands.rs` and seeded into `~/WAID/briefs` on first run),
  so a fresh install should seed neutral placeholder content instead.

Mobile/web versions and any auth/multi-user/sync are explicitly **not** planned —
WAID is single-user, local, desktop-only by design.
