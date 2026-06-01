# WAID — Claude Code Kickoff

**WAID** = "What Am I Doing?" — companion tool to my existing "What's Next?" app. *What's Next?* answers the micro question (what task should I work on right now?); *WAID* answers the macro question (what's the state of all my projects?).

## What we're building

A local-first desktop app to manage my active projects. It's the front door to my workflow: lists projects, renders their context briefs, and lets me kick off tasks in them (open a Claude project, open a repo, fire a webhook). Personal dashboard, not a team tool.

The pattern this replaces: I currently maintain markdown summaries of each project (e.g., `whats_next_project_summary.md`) and paste them into Claude Projects as context. This app is the orchestration layer above that — somewhere to see all the briefs at a glance, edit them, and launch into work.

## Tech stack

- **Tauri 2.x** — desktop shell, Rust backend for native bits
- **SvelteKit** with TypeScript — frontend
- **Tailwind CSS** — styling (current stable)
- **Plain `.md` files on disk** for data — one file per project, YAML frontmatter for metadata, markdown body for the brief

Use current stable versions. Don't pin to old releases without reason.

## Core principles

1. **Local-first means real local-first.** No server, no cloud database, no auth in v1. Just a desktop app reading markdown files from a folder on disk.
2. **Data is portable forever.** Project briefs are plain markdown with YAML frontmatter. If this app disappears tomorrow, the briefs still open in any text editor.
3. **Obsidian-compatible by design.** A future iteration may integrate Obsidian as the editor/storage layer. Don't use any proprietary file format, custom syntax, or schema that would break that path. Standard frontmatter + standard markdown only.
4. **Ship the MVP, iterate fast.** Skip polish, animations, and edge cases until the core loop works.

## Data model

Each project is a single `.md` file in a configurable directory. Default location: a `briefs/` folder either in the user's home directory or in Tauri's app data dir — your call, but make it easy to change later.

Example brief:

```markdown
---
name: Gluefi
status: active
description: Fintech infrastructure for tokenized finance, Japan focus
tags: [fintech, web3, japan]
links:
  - label: Claude Project
    url: https://claude.ai/project/xxx
  - label: GitHub
    url: https://github.com/...
  - label: Notion
    url: https://notion.so/...
webhooks:
  - label: Deploy staging
    url: https://...
    method: POST
last_opened: 2026-05-31T10:00:00Z
---

# Project context

The full markdown brief lives here — background, current state,
decisions log, whatever I want to maintain.
```

Use a standard YAML frontmatter parser (`gray-matter` on the JS side, or `serde_yaml` if parsing in Rust). Don't invent a custom format.

`status` values for v1: `active`, `paused`, `blocked`, `archived`. Don't enforce strictly — render whatever's there as a pill.

## MVP feature set (build in this order)

### 1. Two-pane layout
- Left sidebar: list of all projects parsed from the briefs directory
  - Show name, status pill, last-opened timestamp
  - Click to select
- Main pane: detail view for the selected project
  - Renders the markdown brief
  - Shows links and webhooks as buttons
  - Edit mode toggle

### 2. Markdown rendering
- Use a Svelte-friendly renderer (e.g., `marked` + `DOMPurify`, or `svelte-markdown`)
- Sane defaults; no need for Obsidian-grade features (backlinks, embeds, transclusion)
- Code blocks with syntax highlighting are nice-to-have but not required for v1

### 3. Edit mode
- Toggle between rendered view and a plain textarea for v1
- Save writes back to the `.md` file via Tauri's filesystem API
- No fancy editor yet — upgrading to a proper markdown editor (Tiptap, CodeMirror, Milkdown) is a follow-up

### 4. Action buttons
- Render the `links` array as buttons that open URLs in the default browser (Tauri shell plugin: `@tauri-apps/plugin-shell`)
- Render the `webhooks` array as buttons that fire the request and show a toast on success/failure (Tauri HTTP plugin: `@tauri-apps/plugin-http`)
- Support GET and POST; optional JSON body in frontmatter

### 5. Quick capture
- Global keyboard shortcut (Tauri global-shortcut plugin) opens a small modal anywhere on the system
- Pick a project, type a note, hit save → appends to that project's brief under a `## Captures` heading with a timestamp
- This is the "thought just hit me" path

## Out of scope for v1 (leave TODOs)

- **Obsidian integration** — wire up an Obsidian vault as the storage backend instead of a flat folder. Leave a clear TODO comment near the file-loading logic so this is easy to find.
- **Encrypted secret storage** — when webhooks need auth tokens, use Tauri's secure storage / OS keyring. For v1, plain frontmatter is fine.
- **Search across briefs** — fine to skip at 4–5 projects, becomes useful around 20+.
- **Drag-to-reorder, archive view, status filters** — defer until I actually want them.
- **Mobile / web versions** — desktop only. Don't try to keep these doors open at the cost of v1 simplicity.
- **Auth, multi-user, sync** — never. Single-user, local, by design.

## Suggested project structure

```
waid/
├── src/                      # SvelteKit frontend
│   ├── lib/
│   │   ├── components/
│   │   ├── stores/           # Svelte stores for project state
│   │   └── tauri.ts          # Wrappers around Tauri invoke calls
│   ├── routes/
│   │   ├── +layout.svelte
│   │   └── +page.svelte      # Main app view
│   └── app.html
├── src-tauri/                # Rust backend
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   └── commands.rs       # Tauri commands: read/write/list briefs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── briefs/                   # Sample briefs for dev
├── README.md
└── package.json
```

## Initial tasks for this session

1. Scaffold a new Tauri 2.x + SvelteKit (TypeScript) project
2. Install and configure Tailwind
3. Set up the two-pane layout shell with placeholder content
4. Add a Tauri command (Rust side) that lists all `.md` files in a configurable briefs directory and parses their frontmatter
5. Wire the sidebar to actual files; clicking a project shows its rendered markdown in the main pane
6. Implement edit mode with save-to-file
7. Render `links` as buttons that open in the default browser via the Tauri shell plugin
8. Seed `briefs/` with placeholder briefs for my actual projects so the app has real content on first run:
   - **Gluefi** — fintech / Web2.5 infrastructure for tokenized finance, Japan focus
   - **Blapp** — discovery platform for Black-owned businesses, founded by Jon Laster, currently working on e-commerce
   - **Solaris** — simple mobile game for ad revenue
   - **The Thinking Room** — research into a room/device that pushes users into specific mental states (primarily flow) via music
   - **What's Next?** — Flutter productivity app, weighted random task selection + time-boxing (full summary already exists; this brief can be a short pointer)
9. Commit at logical checkpoints
10. Write a README with run instructions (`pnpm install`, `pnpm tauri dev`, etc.)

Webhooks (#4 above) and quick capture (#5) can be a follow-up session if time runs short. Prioritize getting the read/edit loop solid first.

## Style notes (low priority)

- Tailwind defaults are fine for v1; don't theme yet
- Inter or system fonts; nothing fancy
- Light/dark mode via Tailwind's `dark:` variant + a toggle (nice to have, not required)
- I have a separate Flutter app called "What's Next?" with a distinct floating-rocks/gradient/Nunito aesthetic. **Don't port that here.** This tool should feel utilitarian and fast — no animation budget, no visual flourish. Cohesion across my tools can come later if it makes sense.

## Naming

**WAID** — "What Am I Doing?" Continues the question-as-name pattern from my existing app *What's Next?* Use `waid` as the repo name, package name, and binary name. Display name in the UI can be "WAID" (caps) or stylized however reads well.

---

*This doc is the spec, not the law. If something here turns out to be wrong or awkward in practice, push back — don't just build the broken thing.*
