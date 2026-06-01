# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What WAID is

A local-first **desktop** dashboard (Tauri 2 + SvelteKit) for the macro state of all your projects — "what's the state of everything?" as opposed to *What's Next?*'s "what task now?". Each project is one `.md` file with YAML frontmatter; the app lists them, renders the brief, and lets you launch into work (open links, fire webhooks, quick-capture notes). Single-user, local, desktop-only by design — no server, no cloud, no auth.

## Commands

Run from the repo root (uses **pnpm**):

```bash
pnpm install
pnpm tauri dev      # launch the desktop app with hot reload (frontend + Rust)
pnpm tauri build    # native bundle in src-tauri/target/release/bundle
pnpm check          # type-check frontend (svelte-kit sync && svelte-check)
```

Backend (Rust) tests live in `src-tauri/src/commands.rs`:

```bash
cd src-tauri && cargo test                              # all backend tests
cd src-tauri && cargo test splits_frontmatter_and_body  # a single test by name
```

`pnpm dev` (Vite alone, port 1420) runs the frontend in a browser, but Tauri `invoke` calls fail there — use `pnpm tauri dev` for anything touching the backend. There is no frontend test suite or linter beyond `svelte-check`.

Tauri prerequisites (Rust stable + platform webview deps) are in `README.md`; on Debian/Ubuntu the `apt` line there is required before `tauri dev` will build.

## Architecture

Two halves talking over Tauri's `invoke` bridge:

**Rust backend** (`src-tauri/src/`) — all file I/O and the data model.
- `commands.rs` is the whole backend: brief parsing, the briefs-directory resolution, settings, and every `#[tauri::command]`. `lib.rs` only registers plugins, the global shortcut, and the command handler list; `main.rs` just calls `run()`.
- A **brief** is parsed by `split_frontmatter` (hand-rolled `---` splitter, tolerant of BOM/CRLF and missing closing delimiter) + `serde_yaml`. `FrontMatter` (all fields optional, so half-written files still load) → `Brief` (sent to the frontend, includes both `body` and the full `raw` file).
- **Edits round-trip the whole file**, never a reserialized struct: `save_brief` writes raw text; `touch_brief` and `append_capture` parse to a `serde_yaml::Mapping` / splice text and re-emit, so unknown frontmatter keys and ordering survive. Preserve this property in any new write path — it's what keeps briefs Obsidian-compatible and non-lossy.

**SvelteKit frontend** (`src/`) — Svelte 5 runes, SPA mode (`adapter-static`, `ssr = false` in `+layout.ts`). Tailwind v4 via the Vite plugin.
- `lib/tauri.ts` is the **only** place that names backend commands / channels — thin typed wrappers around `invoke` and the dialog/opener plugins. Components and stores call these, never `invoke` directly. Keep new commands behind a wrapper here.
- `lib/stores/projects.svelte.ts` (`ProjectStore`, a runes class) is the single source of truth for the brief list, selection, and briefs dir. `select()` stamps `last_opened` via `touch_brief` then `upsert`s in place; `upsert` deliberately does **not** reorder the list (sort order comes from the backend on full `load()`).
- `lib/types.ts` mirrors the Rust `Brief`/`Link`/`Webhook` structs — keep the two in sync when changing the data shape (Rust serializes `camelCase`).
- `routes/+page.svelte` is the two-pane shell (`Sidebar` + `ProjectDetail`) and wires quick-capture: an in-window ⌘/Ctrl+K listener plus a `waid://quick-capture` event emitted from Rust when the global Ctrl+Shift+Space hotkey fires.

### Briefs directory (important runtime detail)

The repo's `briefs/*.md` are **seed templates**, not runtime data. They're baked into the binary via `include_str!` (`SEED_BRIEFS` in `commands.rs`). At runtime the app reads from the configured **briefs directory** — default `~/WAID/briefs`, overridable via `set_briefs_dir` (persisted to the app config dir's `settings.json`). On first run, if that directory is empty, the seeds are copied in. Editing the repo's `briefs/` folder does **not** affect a running app unless its briefs dir is pointed there. (Note: the repo seeds still contain real project data — `README.md`'s planned list tracks sanitizing them into generic samples.)

## Conventions

- New backend functionality = a `#[tauri::command]` in `commands.rs` + an entry in the `generate_handler!` list in `lib.rs` + a typed wrapper in `lib/tauri.ts`.
- Frontend uses 2-space indent, double quotes, Svelte 5 runes (`$state`/`$derived`/`$props`). Stores that hold reactive state use the `.svelte.ts` extension.
- The brief format is intentionally standard frontmatter + markdown (Obsidian-native). There's a planned `TODO(obsidian)` in `commands.rs` to back the briefs dir with an Obsidian vault — don't introduce a proprietary format.
