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
- **Secret storage in the OS keyring** — tokens for authenticated integrations
  (e.g. a GitHub token for private-repo brief sync) live in the platform
  keychain, never in settings or env.

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

- **Drag-to-reorder** the project list.
- A richer markdown editor (CodeMirror / Tiptap / Milkdown).
- A dedicated borderless "spotlight" window for quick capture.

Mobile/web versions and any auth/multi-user/sync are explicitly **not** planned —
WAID is single-user, local, desktop-only by design.
