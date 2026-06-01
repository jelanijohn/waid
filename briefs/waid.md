---
name: WAID
status: active
description: Local-first Tauri desktop dashboard for the macro state of all my projects
tags: [tauri, sveltekit, rust, local-first, productivity]
links:
  - label: GitHub
    url: https://github.com/jelanijohn/waid
  - label: Tauri docs
    url: https://v2.tauri.app/
last_opened: 2026-05-31T12:00:00Z
---

# WAID — What Am I Doing?

The **macro** companion to *What's Next?*. Where [[whats-next]] answers "what
task should I work on right now?", WAID answers **"what's the state of all my
projects?"** It lists projects, renders each one's context brief, and lets you
launch into work — open a Claude project, a repo, fire a webhook, or jot a quick
note. Personal tool, not a team tool.

## Current state

- v1 shipped: two-pane layout (sidebar + detail), markdown rendering, edit mode
  with full-file round-trip, link buttons, webhook buttons, quick capture
  (⌘/Ctrl+K + global Ctrl+Shift+Space), light/dark toggle.
- Data is plain `.md` files with YAML frontmatter, read from a configurable
  briefs directory (default `~/WAID/briefs`); seeds samples on first run.
- Stack: Tauri 2 + SvelteKit (Svelte 5, TS, SPA) + Tailwind v4; Rust backend in
  `src-tauri/src/{lib,commands}.rs`.

## Decisions log

- **Local-first, for real.** No server, no cloud DB, no auth — just a desktop
  app reading markdown off disk.
- **Portable forever.** One project = one plain `.md` file; if WAID disappears,
  the briefs still open in any editor.
- **Obsidian-compatible by design.** Standard frontmatter + markdown only, so an
  Obsidian vault can back it later.
- **Utilitarian UI.** No *What's Next?*-style aesthetic; WAID stays plain.

## Open questions

- Obsidian vault as the storage backend (TODO in `commands.rs`).
- Encrypted secret storage for webhook auth tokens (OS keyring).
- Search across briefs, status filters, drag-to-reorder, archive view.
- Richer markdown editor (CodeMirror / Tiptap / Milkdown).

## Out of scope

- Mobile/web versions, auth, multi-user, sync — WAID is single-user, local,
  desktop-only by design.

## Captures
