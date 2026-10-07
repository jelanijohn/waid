# WAID widget mode — implementation spec

Handoff spec for Claude Code. Drop this file in the repo root; delete it once applied.

- **Written against:** `jelanijohn/waid` @ `8297e93` (2026-10-05) and `jelanijohn/whence` @ `1e50341` (2026-08-20), read from the public GitHub repos. Unpushed local work was not seen. If a line number or name below has drifted, trust the code and keep the intent.
- **Visual reference:** the "WAID widget mode" design canvas (five boards: At rest, On focus, A row leafs out, Try it, Alternative · leaf to the side). Claude Code cannot open it, so section 8 restates every metric. Section 8.5 lists where this spec deliberately differs from the mockup.
- **Nothing here has been run.** Section 11 lists what is unverified. Treat those as checks to make, not facts.


## Amendments (HEAD 5b2209e)

Verified against `5b2209e` by reading the code. Where an amendment conflicts with the body below, the amendment wins.

- **A1. Window frame is platform-conditional** (§3.7, §5.5, §5.7, §8.1, §8.5). `src/lib/platform.ts` exports `platform` and `paintsOwnFrame = platform !== "windows"`. `GUTTER = paintsOwnFrame ? 14 : 0`; widget panels take `rounded-[12px] border shadow-[var(--shadow-win)]` only when `paintsOwnFrame`. `LEAF_GAP = paintsOwnFrame ? 10 : 0`: on Windows the side leaf is a second column in the same block, separated by a 1px `--border` divider. macOS detection imports `platform` from `$lib/platform` (no copied regex).
- **A2. Permission ids confirmed** (resolves §11 R1): `core:window:allow-set-size`, `allow-set-min-size`, `allow-set-position`, `allow-set-always-on-top` exist in `gen/schemas/desktop-schema.json`. Getters and `onFocusChanged`/`onMoved` are in `core:default`. `currentMonitor` is a module-level function from `@tauri-apps/api/window`.
- **A3. Refresh citation** (§4.5): brief-level refresh is `ProjectDetail.refresh`; `IntegrationPanel.refresh` is per-feed force fetch (`Fetch failed: …`). Widget leaf refresh = per-feed force fetch (`Fetch failed: …`) + `projects.sync` when `isSyncableBrief` (`Refresh failed: …`). No synthesize, no syncMind.
- **A4.** `page` kind has no special case in IntegrationPanel beyond `kindLabel`; "no count text for `page`" is new widget behaviour.
- **A5.** Brief switching already ends sessions via the `{#key}` remount of `ProjectDetail`. A widget `launch()` on another brief remounts the hidden ProjectDetail (dropping an unsaved draft of the previous brief, same as a sidebar click). The session store's "end the other session silently first" rule stays (idempotent with `onDestroy`).
- **A6. Small facts.** `toasts.push(message, kind, ttl)`; capture toast is `toasts.push("Captured 🎉", "success")`. `projects.markSession` returns `boolean`, never throws. `relativeTime(null)` is `"never opened"`, so `shortAgo` owns its own `—`. `integrations.get()` → `{ loading, error, data }`, `fetchedAt` is `data.fetchedAt`. WSL hint moves into `actions.ts`. Icons are Material Symbols Rounded at wght 400 only.
- **A7. Dev box.** WSLg may run GTK on Wayland, where `setPosition`/`outerPosition`/always-on-top are no-ops; the `canPosition` fallback must work. Test positioning with `GDK_BACKEND=x11 pnpm tauri:wsl`.
- **A8. Window geometry** (§5.5, §5.6, §11 R4/R5):
  - **Enter:** `isMaximized` → if so `toggleMaximize` and poll until un-maximized (≤300ms) → then read `innerSize`/`outerPosition`/`scaleFactor` (logical) → `setMinSize(ROSTER_W+2G × 60+2G)` → `setAlwaysOnTop(true)` → `setSize(final)` → position.
  - **First size is final:** the store sets `mode="widget"; shifting=true`, awaits `tick()`, measures the mounted (hidden) shell, then calls `enterWidgetWindow(anchor, measuredH)`. One resize, no 100px placeholder.
  - **Exit:** `setAlwaysOnTop(false)` → `setSize(saved)` → `setPosition(saved)` (only if `canPosition`) → `setMinSize(720×480)` → `toggleMaximize()` if it was maximized.
  - **`setWidgetSize`:** dedupe on the last *requested* physical size, serialize through one in-flight promise, send `PhysicalSize(ceil(w·sf), ceil(h·sf))`, re-apply on `onScaleChanged`.
  - **Side leaf order** stays: open-left = move then widen; close-left = shrink then move; `shifting` held until both resolve + one rAF. R4's "try reversing" dropped.
  - **R5 is moot** (Windows relies on the DWM shadow; Linux has none). No `setShadow`.
  - **Focus:** both `win.onFocusChanged` and DOM focus/blur feed an idempotent `setFocused`; init from `document.hasFocus()`; the grace timer re-checks `document.hasFocus()` before collapsing; `pointerdown`/`keydown` in the shell count as focused.
  - **Position probe:** `moveTo` = `setPosition` then poll `outerPosition` (50ms × 6) for `|Δ| ≤ ceil(sf)+1` physical px; failure sets `canPosition=false` (leaf opens right, no shift, no position memory, exit skips `setPosition`). Guard `currentMonitor()` → `null`.
  - Resizable stays on; a user edge-drag leaves a wrong size until the next content change. Acceptable v1.
- **A9. Found in testing (Xvfb, X11).** `currentMonitor()` / `monitorFromPoint()` are unsafe on Linux with `tauri-runtime-wry` 2.11.2: the `gdk::Monitor` is fetched on the main thread but its geometry is read on the IPC thread, which corrupts the X connection, and the next window call aborts (`xcb_xlib_threads_sequence_lost`) or hangs. On Linux, `widgetWindow.ts` reads the work area from DOM `screen.avail*` and clamps the saved anchor into it instead of the monitor lookup. Windows and macOS keep the Tauri monitor APIs.
- **A10. Wake click.** The roster renders on pointerdown (or the OS focus event just before it), so the click that wakes the widget from rest would also land on the row or button rendered under the pointer. `WidgetShell` swallows a click whose press began at rest or within 350ms of waking.

---

## 1. What this adds

A second presentation of the same app window: a small always-on-top widget modelled on Whence's roster (`whence/src/routes/+page.svelte`, `ProjectRow.svelte`).

Three states, each a superset of the one before:

1. **Rest** (window not focused). One line that answers the app's name: the current brief, its status, and its `## Activity` summary, plus a status tally of the other briefs.
2. **Roster** (window focused). One 30px row per non-archived brief, with quick capture pinned at the bottom.
3. **Leaf** (a row opened). That brief's Current State, a one-line count per feed, its launch links and webhooks, session control, and "Open brief".

The leaf has two presentations, chosen by a new setting (`settings.widgetLeaf`):

- **`accordion`** (default): the leaf opens inline under its row and the window grows downward.
- **`side`**: the leaf opens as a second panel beside the roster and the window grows sideways.

"Open brief" and the expand button return to the full dashboard.

---

## 2. Charter checklist

Every item must hold when the work is done.

- [ ] **No Rust changes.** No new `#[tauri::command]`, nothing in `src-tauri/src/**`. `cargo test` output is unchanged.
- [ ] **No new dependencies**, npm or cargo. (No `tauri-plugin-window-state`; position is persisted in the existing `waid-settings` localStorage blob.)
- [ ] **Rest and roster read only `projects.briefs`**, which is already in memory. Zero network, zero keyring, zero disk writes to render them.
- [ ] **No polling.** No timer fetches anything. Feeds are fetched lazily when a leaf opens (cached by `integrations.fetch`) and on the leaf's refresh button. This is the existing rule in `stores/integrations.svelte.ts`, with "leaf open" standing in for "brief open".
- [ ] **No new write paths.** The widget can reach only writes that already exist: `append_capture`, `touch_brief` (through `projects.select`), `sync_brief`, `sync_all` / `synthesize_all` (header button, same as the titlebar), `mark_brief_session`, and `fire_webhook` (HTTP only).
- [ ] **Peeking does not write.** Opening a leaf never calls `projects.select`, so it never stamps `last_opened`.
- [ ] **Brief text is data.** Everything the widget shows from a brief body is rendered through Svelte text interpolation. Never `{@html}`, never `renderMarkdown`.
- [ ] **Dashboard parity.** With widget mode never entered, the dashboard behaves exactly as before (Phase 0 is a pure refactor).
- [ ] **No resize feedback loop.** Widget layout never depends on window size: fixed pixel widths, auto heights, no `vh`/`vw`/`%` of the window.
- [ ] **Clean exit.** Leaving widget mode restores size, position, maximized state, the 720×480 minimum, and always-on-top off.

---

## 3. Decisions taken (and why)

These are settled for this spec. Section 12 preserves the ones worth revisiting.

### 3.1 One window, two modes. No second window.

Widget mode resizes and re-flags the existing `main` window. It does not create a `widget` window.

- Whence's settings popup (`whence/docs/settings-popup-window-plan.md`) shows the cost of a second window: stores are per JS realm, so every shared value needs an event channel. WAID's widget needs `projects`, `integrations`, `toasts`, `settings`, and session state. One realm shares all of them for free.
- A second label also needs its own capability entry, and a missing one fails silently (that plan's step 3).
- Cost: the dashboard is not visible while the widget is. That matches "a mode".

### 3.2 The dashboard stays mounted, hidden with `display: none`.

`+page.svelte` wraps the existing shell in a container that gets `hidden` in widget mode. It is not unmounted.

- `ProjectDetail` keeps an unsaved edit draft, scroll position, and its `onDestroy` session safety net. Unmounting on a mode switch would drop the draft and end the session.
- Cost: global listeners keep running. There are exactly three to guard (section 6, `Sidebar.svelte` and `+page.svelte`).
- Checked: `ProjectDetail`'s `railCap` ResizeObserver only derives a value; it never writes `settings.railWidth`, so a zero-size hidden pane cannot corrupt the saved rail width.

### 3.3 Window API calls live in the frontend.

`Titlebar.svelte` and `ResizeHandles.svelte` already call `getCurrentWindow()` directly. Widget mode follows that precedent in one new module, `src/lib/widgetWindow.ts`. `lib/tauri.ts` stays the only place that names backend commands, and gains none.

### 3.4 Session state moves to a store.

`sessionActive` is local to `ProjectDetail` (lines 235–282). The widget must show "· in session" and offer End session, so the state is lifted into `src/lib/stores/session.svelte.ts`. Section 5.1.

### 3.5 The "lead" brief is `projects.selected`.

The brief shown at rest and pinned first in the roster is the dashboard's current selection, falling back to the first roster entry. The selection survives the mode switch because the store is shared.

### 3.6 Helpers that parse the body live in TypeScript.

`activityLine` and `currentStateExcerpt` read `brief.body` in the frontend, the same way the Obsidian wiring is "purely frontend". Adding fields to the Rust `Brief` would be fixture-testable but touches `commands.rs`, `types.ts`, and every `Brief` consumer. There is no frontend test runner, so section 9 gives input/expected pairs to check by hand.

### 3.7 The widget keeps the dashboard's 14px transparent gutter.

Whence paints its panel edge to edge with `"shadow": false` in `tauri.conf.json`. WAID has no such flag and already runs with a 14px gutter (`+page.svelte`, `p-[14px]`). Reusing it means any native shadow or border behaves exactly as it does today, on every platform Jelani already runs. Window size is therefore content size + 28 in each dimension.

---

## 4. Behavior

### 4.1 States and transitions

```
dashboard ──[Titlebar "Widget mode" button]──▶ widget/roster
widget/roster ──[window blur, 200ms grace]──▶ widget/rest
widget/rest ──[window focus]──▶ widget/roster
widget/roster ──[click row]──▶ widget/leaf(path)
widget/leaf(path) ──[click same row | Esc]──▶ widget/roster
widget/leaf(path) ──[click other row]──▶ widget/leaf(other)
widget/leaf ──[window blur, 200ms grace]──▶ widget/rest   (leaf closes)
widget/* ──[expand button]──▶ dashboard                    (selection unchanged)
widget/leaf(path) ──[Open brief]──▶ dashboard              (after projects.select(path))
```

- Entering widget mode lands in **roster**, because the click that entered it leaves the window focused.
- Blur uses a 200ms grace timer. A refocus inside the window cancels it.
- Opening a link or firing a webhook blurs the window, which collapses it to rest. That is intended: you launched into work.
- `widget.captureDraft` survives a collapse. Refocusing shows the unsent text.

### 4.2 Rest

```
┌──────────────────────────────────────────┐
│ W  WAID                      ●3  ●1  ●1  │  header (drag region), tally right
│ Whence · in session    ● ACTIVE     now  │  lead row
│ 2 open PRs · 5 open issues · CI ✓ · la…  │  activity line (only if present)
└──────────────────────────────────────────┘
```

- The whole panel below the header is one `<button>`. Clicking it focuses the window, which opens the roster. No other controls render at rest.
- Tally: one dot + count per known status with a non-zero count, in `STATUS_ORDER`, over non-archived briefs. `archived` and custom statuses are not counted.
- "· in session" shows only when `session.isActive(lead.path)`.
- No briefs: the lead row reads "No briefs yet" in `--fg2`, no tally.

### 4.3 Roster

```
┌──────────────────────────────────────────┐
│ W  WAID                          ⟳   ⤢  │  sync all · expand to dashboard
│ Whence · in session  ● ACTIVE   now   ⌄  │  lead row (bold)
│ 2 open PRs · 5 open issues · CI ✓ · la…  │
│ Wollabo Site         ● ACTIVE   now   ⌄  │
│ NeuroSkill           ● ACTIVE   11h   ⌄  │
│ Glue                 ● BLOCKED   6d   ⌄  │
│ Who Am I             ● PAUSED    1w   ⌄  │
├──────────────────────────────────────────┤
│ ✎  Capture a note to Whence       Ctrl K │
└──────────────────────────────────────────┘
```

- **Rows:** `widget.roster` = the lead first, then `projects.briefs` in store order (the backend's manual order or recency sort), excluding `status === "archived"` (case-insensitive) unless that brief is the lead. Sidebar `query` and `statusFilter` are ignored.
- **Row content:** name · status dot + label · `shortAgo(brief.lastOpened)` · chevron. The whole row is one `<button>` that toggles the leaf.
- **More than 8 rows:** the list gets `max-height: 240px` and `overflow-y: auto` with the existing `scroll-thin` class.
- **Header buttons:** Sync all (calls the shared `syncAllProjects()`, section 5.2) and Expand (`widget.exit()`). There is no close button: closing would quit the app, and the taskbar and dashboard caption buttons already do that.
- **Capture:** a single-line `<input>` with a visually hidden `<label>`. Enter submits to the open leaf's brief if a leaf is open, else the lead: `appendCapture(path, text)` → `projects.upsert(updated)` → `toasts.push("Captured 🎉", "success")` (the string `QuickCapture.svelte` uses). Empty input is a no-op. The hint reads `⌘K` on macOS, `Ctrl K` elsewhere.

### 4.4 Leaf content (both presentations)

Rendered by one component, `WidgetLeaf.svelte`, with a `variant` prop. Blocks, top to bottom; each is omitted when it has nothing to show:

1. **Summary.** Label "Current state" with `currentStateExcerpt(brief.body)`. If that is null, label "About" with `brief.description`. Accordion clamps to 4 lines; side clamps to 8.
2. **Live state.** One row per entry in `brief.integrations`: `ProviderTile` (size 16) · `kindLabel(ig.kind, conn.provider)` · right-aligned count text (rule in 4.5). Header right side: "as of {relativeTime(oldest fetchedAt)}" and a refresh button.
3. **Launch.** A wrapping row of chips: each `brief.links` entry, then each `brief.webhooks` entry in the `--hook-*` colors with its method. Same order and labels as `ProjectDetail`'s launch row (starting at line 663).
4. **Actions.** Left: Start session / End session, only when the brief has a `neuroskill` connection. Right: "Open brief".

Side variant additions: a title block above block 1 (status dot, brief name at 15px/600, description in `--fg2`), "Open brief" moves into that title row, and each feed row is followed by up to 2 items (`item.title` truncated, `relativeTime(item.updatedAt)`), each a button that calls `openExternal(item.url)`.

### 4.5 Feed rows: what they show and when they fetch

- **On leaf open:** for each `ig` in `brief.integrations` with `ig.kind !== "mind"`, call `integrations.fetch(brief.path, ig.connection, ig.kind, ig.query, ig.limit).catch(() => {})`. This is the same call `IntegrationPanel`'s `onMount` makes; it returns the cached result without a network call when one exists.
- **Count text**, from `integrations.get(...)`:
  - `entry.data` present, exactly one key in `summary.byStatus` → `"{n} {status lowercased}"` (e.g. `2 open`).
  - `entry.data` present otherwise → `"{total} item"` / `"{total} items"`.
  - `ig.kind === "page"` → no count text.
  - `ig.kind === "mind"` → the rolling window: the last whitespace-separated token of `ig.query` that is `today`, `7d`, `14d` or `30d` (case-insensitive), else `14d`. The query can also carry a `slug:` token (`neuroskill::parse_mind_query`), which is not shown.
  - `entry.loading` with no data → `…`. `entry.error` with no data → `—` with the error as `title`.
- **Refresh button:** force-fetch every non-`mind` feed (`force = true`), and call `projects.sync(brief.path)` when `isSyncableBrief(brief)`. Errors toast as `IntegrationPanel.refresh` does. It does **not** call `synthesize` or `syncMind`: LLM calls and Mind State writes stay in the dashboard.

### 4.6 Launching from a leaf

Launching is "this is what I'm doing now", so it selects the brief first:

```
launch(brief, action):
  if projects.selectedPath !== brief.path: await projects.select(brief.path)
  action()        // launchLink / launchWebhook / session.start
```

`launchLink` and `launchWebhook` start a session as a side effect, exactly as `ProjectDetail.openLink` / `fire` do today.

### 4.7 Quick-capture shortcuts in widget mode

Both triggers in `+page.svelte` (the `waid://quick-capture` event and in-window ⌘/Ctrl+K) call `widget.requestCaptureFocus()` instead of setting `captureOpen = true`. The Rust handler already shows and focuses the window, which opens the roster; the shell then focuses the capture input. The `QuickCapture` modal never opens in widget mode.

### 4.8 Keyboard

- Rows and chips are native buttons and links, so Tab order works without extra code.
- `Esc`: closes an open leaf. With no leaf open it does nothing.
- No arrow-key navigation in this version.

---

## 5. New modules

### 5.1 `src/lib/stores/session.svelte.ts` (new)

Lifts lines 235–282 of `ProjectDetail.svelte`. Toast strings are copied verbatim from there.

```ts
class SessionStore {
  /** Path of the brief with an open labeled session, or null. At most one. */
  activePath = $state<string | null>(null);
  private warned = false;

  isActive(path: string): boolean;

  /** No-op when the brief has no neuroskill connection or is already active.
   *  If another brief's session is open, end it silently first. Optimistically
   *  set activePath, then roll back and warn (once per daemon-down stretch) if
   *  projects.markSession(path, "start") returns false. */
  start(brief: Brief): Promise<void>;

  /** Clear activePath and fire the end label. `silent` suppresses the
   *  "session end wasn't recorded" toast (used by onDestroy). */
  end(opts?: { silent?: boolean }): void;
}
export const session = new SessionStore();
```

One behavior difference to accept: `neuroWarned` was per component instance, so it reset on every brief switch. `warned` is now app-wide, so the "couldn't reach NeuroSkill" toast appears once per outage instead of once per brief.

### 5.2 `src/lib/actions.ts` (new)

Three functions extracted so the dashboard and the widget cannot drift.

```ts
/** ProjectDetail.openLink: session.start(brief), then openExternal(url);
 *  on failure toasts `Could not open link (${e}).` plus the WSL hint when
 *  projects.isWsl. */
export async function launchLink(brief: Brief, url: string): Promise<void>;

/** ProjectDetail.fire: session.start(brief), then fireWebhook(brief.path, hook)
 *  with the same toasts. */
export async function launchWebhook(brief: Brief, hook: Webhook): Promise<void>;

/** Titlebar.syncAll's body, including the synthesizeAll step when
 *  projects.llmProvider is set and all of its toasts. The caller owns
 *  its own `syncing` flag. */
export async function syncAllProjects(): Promise<void>;
```

### 5.3 `src/lib/widget.ts` (new, pure)

No Tauri, no DOM, no stores. Marker strings mirror `SYNC_START` / `SYNC_END` and `marker_start("waid:state")` in `commands.rs`; add a comment saying so.

```ts
/** Detail of the first OK source line in the managed sync block, or null. */
export function activityLine(body: string): string | null;

/** Plain-text first paragraph of the waid:state region, or null. */
export function currentStateExcerpt(body: string): string | null;

/** Counts for active/paused/blocked over non-archived briefs, zero counts dropped. */
export function statusTally(briefs: Brief[]): { status: string; count: number }[];

/** Rule from 4.5. */
export function feedCountText(ig: BriefIntegration, entry: { loading: boolean; error: string | null; data: IntegrationFetch | null } | null): string;
```

`activityLine` rules:

1. Take the text between `<!-- waid:sync:start -->` and `<!-- waid:sync:end -->`. Missing or reversed markers → null.
2. For each line that starts with `**`: find the **closing** `**`. Do not split on the first ` · `: labels contain one (`collect_source_results` writes `GitHub · {owner}/{repo}`).
3. The remainder after the closing `**` must start with ` · `. Skip the line if it does not (a label-only line) or if the detail starts with `⚠️` (an error line).
4. Split the detail on ` · `, drop any segment starting with `last push ` (it is relative to sync time and goes stale), rejoin with ` · `.
5. Return the first non-empty result, else null.

`currentStateExcerpt` rules: take the text between `<!-- waid:state:start -->` and `<!-- waid:state:end -->`; drop the leading `## Current State` line; take the first non-empty paragraph; strip `**`, `__`, backticks, and reduce `[text](url)` and `[[target|alias]]` / `[[target]]` to their text; collapse whitespace; empty → null.

### 5.4 `shortAgo` in `src/lib/time.ts` (added export)

`shortAgo(iso?: string | null): string` → `—` for null or unparseable, `now` under 60s, then `5m`, `11h`, `6d`, `1w`, `3mo`, `2y`, using the same unit ladder as `relativeTime`. `relativeTime` is unchanged.

### 5.5 `src/lib/widgetWindow.ts` (new)

All `@tauri-apps/api/window` calls for widget mode. Every function catches and no-ops when the window API is unavailable (plain `vite dev`), as `Titlebar.svelte` does.

```ts
export const ROSTER_W = 300;
export const LEAF_W = 340;
export const LEAF_GAP = 10;
export const GUTTER = 14;        // matches +page.svelte's p-[14px]
export const EDGE_MARGIN = 24;
const DASH_MIN = { w: 720, h: 480 };   // tauri.conf.json minWidth / minHeight

export interface SavedGeometry { x: number; y: number; w: number; h: number; maximized: boolean } // logical px

/** Returns the dashboard geometry to restore later (null if unavailable). */
export async function enterWidgetWindow(anchor: { x: number; y: number } | null): Promise<SavedGeometry | null>;
export async function exitWidgetWindow(saved: SavedGeometry | null): Promise<void>;
/** Content size in logical px; adds 2 × GUTTER itself. */
export async function setWidgetSize(contentW: number, contentH: number): Promise<void>;
/** Move the window horizontally; resolves false if the position did not change. */
export async function shiftWindowX(dx: number): Promise<boolean>;
/** "left" when the roster's centre is in the right half of its monitor, else "right". */
export async function pickLeafSide(): Promise<"left" | "right">;
/** Current window top-left in logical px, or null. */
export async function windowPosition(): Promise<{ x: number; y: number } | null>;
```

`enterWidgetWindow`, in order:

1. `maximized = await win.isMaximized()`; if true, `await win.toggleMaximize()` (already permitted).
2. Read `innerSize()`, `outerPosition()`, `scaleFactor()`; convert to logical; keep as `SavedGeometry`.
3. `setMinSize(new LogicalSize(200, 60))`. This must come before the shrink, or the 720×480 minimum clamps it.
4. `setAlwaysOnTop(true)`.
5. `setSize(new LogicalSize(ROSTER_W + 2 * GUTTER, 100 + 2 * GUTTER))`. The shell's ResizeObserver corrects the height on the next frame.
6. Position: `anchor` if given; otherwise top-right of `currentMonitor()` with `EDGE_MARGIN`. Read the position back; if it did not change, set a module flag `canPosition = false` (see 11, R3).

`exitWidgetWindow`, in order: `setAlwaysOnTop(false)` → `setSize(saved.w, saved.h)` → `setMinSize(new LogicalSize(720, 480))` → `setPosition(saved.x, saved.y)` → if `saved.maximized`, `toggleMaximize()`. With `saved === null`, only the first and third run.

`skipTaskbar` and `resizable` are deliberately not touched.

### 5.6 `src/lib/stores/widget.svelte.ts` (new)

```ts
export type WidgetMode = "dashboard" | "widget";

class WidgetStore {
  mode = $state<WidgetMode>("dashboard");
  focused = $state(true);
  openPath = $state<string | null>(null);
  leafSide = $state<"left" | "right">("left");   // meaningful for the side variant only
  captureDraft = $state("");
  captureFocusTick = $state(0);
  /** True while a side-leaf reposition is in flight; the shell hides content. */
  shifting = $state(false);
  /** Registered by WidgetShell on mount, cleared on destroy: the measured
   *  element's current content height in logical px. */
  measure: (() => number) | null = null;

  get lead(): Brief | null;      // projects.selected ?? first non-archived brief ?? null
  get roster(): Brief[];         // rule in 4.3
  get openBrief(): Brief | null; // roster entry for openPath

  enter(): Promise<void>;        // saved = await enterWidgetWindow(settings.widgetPos), kept privately for exit; mode = "widget"; focused = true
  exit(selectPath?: string): Promise<void>; // close leaf, optional projects.select, exitWidgetWindow, mode = "dashboard"
  setFocused(f: boolean): void;  // false → 200ms timer → focused = false + closeLeaf(); true cancels the timer
  toggleLeaf(path: string): Promise<void>;
  closeLeaf(): Promise<void>;
  requestCaptureFocus(): void;   // captureFocusTick++
}
export const widget = new WidgetStore();
```

`toggleLeaf` / `closeLeaf` with `settings.widgetLeaf === "accordion"` only set `openPath`. With `"side"`:

```
open (no leaf currently open):
  leafSide = canPosition ? await pickLeafSide() : "right"
  shifting = true
  if leafSide === "left":
    ok = await shiftWindowX(-(LEAF_W + LEAF_GAP))
    if (!ok) leafSide = "right"
  openPath = path
  await tick(); await setWidgetSize(ROSTER_W + LEAF_GAP + LEAF_W, this.measure?.() ?? 0)
  shifting = false

close:
  shifting = true
  openPath = null
  await tick(); await setWidgetSize(ROSTER_W, this.measure?.() ?? 0)
  if leafSide === "left": await shiftWindowX(+(LEAF_W + LEAF_GAP))
  shifting = false

switch (leaf open, different row): openPath = path     // no geometry change
```

Opening to the right needs no reposition: the window only widens.

### 5.7 Components (new)

- **`WidgetShell.svelte`**: the widget root. Owns the focus listener, the ResizeObserver, the moved listener, and the layout of roster + side leaf.
- **`WidgetRow.svelte`**: one roster row. Props: `brief`, `lead: boolean`, `inSession: boolean`, `open: boolean`, `side: "left" | "right" | null`, `onToggle`.
- **`WidgetLeaf.svelte`**: leaf content. Props: `brief`, `variant: "accordion" | "side"`.

`WidgetShell` structure:

```
<div class="fixed inset-0 overflow-hidden p-[14px]">            ← never measured
  <div bind:this={measured}                                      ← measured; width from state, height auto
       class="flex items-start gap-[10px]"
       style="width: {contentW}px; visibility: {widget.shifting ? 'hidden' : 'visible'}">
    {#if side variant && openBrief && leafSide === 'left'}  <SidePanel/>  {/if}
    <RosterPanel/>           ← 300px; rest or roster by widget.focused
    {#if side variant && openBrief && leafSide === 'right'} <SidePanel/>  {/if}
  </div>
</div>
```

- `contentW` = `ROSTER_W`, plus `LEAF_GAP + LEAF_W` when the side leaf is open.
- **Sizing effect:** a ResizeObserver on `measured` calls `setWidgetSize(contentW, Math.ceil(height))`, coalesced with `requestAnimationFrame`. It is skipped while `widget.shifting`.
- **Focus:** on mount, `win.onFocusChanged(({ payload }) => widget.setFocused(payload))`; unlisten on destroy.
- **Position memory:** on mount, `win.onMoved(...)` debounced 400ms and ignored while `widget.shifting`. It stores the **roster anchor**, not the window origin: `x = windowX + (side leaf open on the left ? LEAF_W + LEAF_GAP : 0)`, `y = windowY`, via `settings.setWidgetPos`.
- **Drag:** both panel headers carry `data-tauri-drag-region` (`core:window:allow-start-dragging` is already granted).
- **macOS:** the window keeps its native traffic lights (`titleBarStyle: "Overlay"`). Reserve an 80px inset at the left of the roster header there, using the same `userAgent` sniff and the `.mac-inset` idea from `Titlebar.svelte`. Do not toggle decorations.
- `ResizeHandles` is not rendered in widget mode.

---

## 6. Blast radius

### New files (8)

| File | Purpose |
|---|---|
| `src/lib/stores/session.svelte.ts` | 5.1 |
| `src/lib/actions.ts` | 5.2 |
| `src/lib/widget.ts` | 5.3 |
| `src/lib/widgetWindow.ts` | 5.5 |
| `src/lib/stores/widget.svelte.ts` | 5.6 |
| `src/lib/components/WidgetShell.svelte` | 5.7 |
| `src/lib/components/WidgetRow.svelte` | 5.7 |
| `src/lib/components/WidgetLeaf.svelte` | 5.7 |

### Modified files (8)

| File | Change |
|---|---|
| `src/lib/components/ProjectDetail.svelte` | Replace local `sessionActive` / `neuroWarned` / `startSession` / `endSession` with the session store (`sessionActive` becomes `$derived(session.isActive(brief.path))`). `onDestroy` becomes `if (session.isActive(brief.path)) session.end({ silent: true })`. `openLink` and `fire` delegate to `launchLink` / `launchWebhook`. Nothing else. |
| `src/lib/components/Titlebar.svelte` | `syncAll` keeps its `syncing` flag and calls `syncAllProjects()`. Add a "Widget mode" `btn-icon` in `.actions` before `<AppMenu />` (`Icon name="picture_in_picture_alt"`, `title` and `aria-label` "Widget mode", `onclick={() => widget.enter()}`). |
| `src/lib/components/Sidebar.svelte` | First line of the `onKey` handler in `onMount`: `if (widget.mode === "widget") return;`. Without it ⌘F, ⌘N and Alt+Arrow act on the hidden sidebar. |
| `src/routes/+page.svelte` | Wrap the existing shell `div.h-screen…` in a container with `class:hidden={widget.mode === "widget"}`. Render `<ResizeHandles />` only in dashboard mode. Render `<WidgetShell />` in widget mode. Route both quick-capture triggers per 4.7. `QuickCapture` and `Toasts` stay where they are. |
| `src/lib/stores/settings.svelte.ts` | Add `export type WidgetLeaf = "accordion" \| "side"`; fields `widgetLeaf` (default `"accordion"`) and `widgetPos: { x: number; y: number } \| null` (default `null`) to `Persisted`, the class, `init()`, and `persist()`; setters `setWidgetLeaf` and `setWidgetPos`. Validate `widgetPos` on load (two finite numbers, else `null`). |
| `src/lib/components/SettingsModal.svelte` | Appearance section, directly after the "Brief layout" block (line ~515): a "Widget detail" segmented control using the identical markup, driven by `const WIDGET_LEAVES: { value: WidgetLeaf; label: string }[] = [{ value: "accordion", label: "Accordion" }, { value: "side", label: "Side leaf" }]` and `settings.setWidgetLeaf`. |
| `src/lib/time.ts` | Add `shortAgo` (5.4). |
| `src-tauri/capabilities/default.json` | Add `core:window:allow-set-size`, `core:window:allow-set-min-size`, `core:window:allow-set-always-on-top`, `core:window:allow-set-position`. |

### Do not touch

- `src-tauri/src/**` and `src-tauri/Cargo.toml`: no Rust changes of any kind.
- `src-tauri/tauri.conf.json`: the window's config-time size and minimums stay as they are; widget mode overrides them at runtime and restores them.
- `src/lib/types.ts`: no data-shape change.
- `src/lib/tauri.ts`: no new commands. Import existing wrappers only.
- `src/lib/components/IntegrationPanel.svelte`, `IntegrationsModal.svelte`, `QuickCapture.svelte`, `MarkdownView.svelte`, `src/lib/markdown.ts`, `src/lib/stores/integrations.svelte.ts`, `src/lib/stores/projects.svelte.ts`: read and call, do not edit.
- `src/app.css`: no new tokens. The widget uses existing variables and Tailwind arbitrary values.
- `briefs/` seeds and `package.json`.

### Read first

1. `whence/src/routes/+page.svelte` and `whence/src/lib/components/ProjectRow.svelte`: the row and dynamic-resize pattern being borrowed.
2. `whence/docs/settings-popup-window-plan.md`: why a second window was not chosen here.
3. `src/routes/+page.svelte`, `src/lib/components/Titlebar.svelte`, `ResizeHandles.svelte`: the window shell.
4. `src/lib/components/ProjectDetail.svelte` lines 235–300 (session, `openLink`, `fire`) and the launch row markup starting at line 663.
5. `src/lib/components/IntegrationPanel.svelte` lines 60–125: feed fetch, `summaryLine`, `refresh`.
6. `src/lib/stores/projects.svelte.ts`, `settings.svelte.ts`, `integrations.svelte.ts`.
7. `src-tauri/src/commands.rs`: `render_sync_summary` (393), `collect_source_results` (1306), `format_github_detail` (1103), the `waid:state` write in `synthesize_brief` (2273). Read only; these define the text 5.3 parses.

---

## 7. Phases

Each phase leaves the app working. Commit per phase.

**Phase 0 — refactor, no visible change.** `session.svelte.ts`, `actions.ts`, and the `ProjectDetail` / `Titlebar` edits that use them. Verify: `pnpm check`; in the dashboard, Start/End session, launch-starts-session, switching briefs ends the session, Sync all toasts — all as before.

**Phase 1 — widget window, rest, roster, capture.** `widgetWindow.ts`, `widget.svelte.ts`, `widget.ts` (`activityLine`, `statusTally`), `shortAgo`, `WidgetShell`, `WidgetRow`, capabilities, the `+page` / `Sidebar` / `Titlebar` edits, and the `widgetPos` half of the settings change. Rows do not open yet.

**Phase 2 — accordion leaf.** `WidgetLeaf` with `variant="accordion"`, `currentStateExcerpt`, `feedCountText`, leaf-open fetch, refresh, launch, session, Open brief.

**Phase 3 — side leaf and the setting.** `settings.widgetLeaf`, the `SettingsModal` control, `variant="side"`, `pickLeafSide`, `shiftWindowX`, the `shifting` handshake, roster-anchor position memory.

---

## 8. Visual spec

All colors are existing `app.css` variables, so light, dark and the accent setting work without new CSS. Font is the app's Inter.

### 8.1 Panels

- Roster panel: width 300. Side leaf panel: width 340, padding 14, internal gap 14. Gap between the two: 10.
- Both use the dashboard panel's classes: `rounded-[12px] border border-[var(--border)] bg-[var(--bg)] text-[var(--fg)] shadow-[var(--shadow-win)]`, plus `overflow-hidden`.

### 8.2 Roster

| Element | Spec |
|---|---|
| Header | padding `12px 12px 8px`; brand: `BrandMark size={18}` + "WAID" 15px/500 `--fg-body`, gap 8 |
| Tally (rest) | gap 12; each: 7px dot (`statusColor`) + count, 12px `--fg2`, tabular-nums, gap 5 |
| Header buttons (roster) | 24×24, icon 16px, `--fg2`, hover `--fg`; `sync` (add `spin` while syncing), `open_in_full` |
| Row | height 30, padding `4px 12px`, gap 8, hover `--hover` |
| Name | 14px; lead: 600 `--fg`; others: 400 `--fg2`; truncate |
| "· in session" | 11px `--fg2`, lead row only |
| Status | 7px dot + label 10px/600 uppercase, letter-spacing 0.08em, `--fg2`, gap 6; text = `STATUS_LABEL[status] ?? status`; nothing when the brief has no status |
| Time | 13px `--fg2`, tabular-nums, width 26, right-aligned |
| Chevron | 16px icon in a 20px box; closed `expand_more` `--fg2`; open `expand_less` `--accent`. Side variant, open: `chevron_left` or `chevron_right` pointing at the leaf, and the row gets `background: var(--sel)` |
| Activity line | lead only; padding `0 12px 6px`; 11px/14px `--fg2`; single line, truncate; no reserved height when null |
| Capture footer | `border-top: 1px solid var(--border)`; padding `8px 12px`; gap 8; `edit` icon 16px `--fg2`; input 12px `--fg-body`, no border, transparent; placeholder `--fg2`; key hint 10px/600 `--fg2` in a 1px `--border` box, radius 5, padding `2px 5px` |

### 8.3 Accordion leaf

- Band directly under the row: `background: var(--row-alt)`, 1px `--border` top and bottom, padding 12, block gap 14, `max-height: 320px; overflow-y: auto` (`scroll-thin`).
- Section label: 10px/600 uppercase, 0.08em, `--fg2`. Summary text: 12px/17px `--fg-body`, `-webkit-line-clamp: 4`.
- Feed row: height 24, 12px; `ProviderTile size={16}`, label `--fg-body`, count `--fg2` tabular right-aligned. "as of …": 11px `--fg2`; refresh button 20×20, `sync` icon 14px.
- Chips: height 26, padding `0 9px`, radius 7, 1px `--border`, 12px `--fg-body`, trailing `north_east` 12px; wrap with gap 6. Webhook chips: `--hook-bd` / `--hook-bg` / `--hook-fg`, leading `bolt` (fill 1), method at 9px/600 uppercase.
- Actions row: session button 12px `--fg-body` with `neurology` (start) or `stop_circle` fill 1 (end); "Open brief" 12px/500 `--accent` with `north_east`.

### 8.4 Side leaf

Same blocks inside the 340px panel, without the band background. Title row: 8px status dot + name 15px/600 `--fg`, "Open brief" right-aligned; description 12px/17px `--fg2`. Feed rows use label weight 500 `--fg`. Item rows: height 22, indented 24px, title `--fg-body` truncate, time `--fg2`.

### 8.5 Where this differs from the mockup, on purpose

- Corner radius is 12 (the dashboard panel), not 14, and there is no large drop shadow: the 14px gutter would clip it.
- No ✕ button in the roster header.
- The activity line is the real sync text: `CI ✓` and `latest v1.5.2`, where the mockup wrote "CI passing" and "v1.5.2".
- Feed counts follow 4.5, so they may read "2 items" where the mockup shows "2 open".
- Secondary text uses `--fg2` throughout. `--fg3` is too faint at 10–11px.

---

## 9. Helper acceptance examples

No frontend test runner exists, so check these by hand (a scratch `console.assert` block that is not committed is fine).

`activityLine`:

| Body contains (inside `waid:sync` markers) | Result |
|---|---|
| `**GitHub · jelanijohn/whence** · 2 open PRs · 5 open issues · last push 3h ago · CI ✓ · latest v1.5.2` | `2 open PRs · 5 open issues · CI ✓ · latest v1.5.2` |
| `**CI** · ⚠️ returned 500` then `**GitHub · a/b** · 1 open PR` | `1 open PR` |
| `**GitHub · a/b**` (label only) | `null` |
| `**GitHub · a/b** · last push 2d ago` | `null` |
| no markers, or end marker before start marker | `null` |

`currentStateExcerpt`:

| Body contains | Result |
|---|---|
| `<!-- waid:state:start -->\n## Current State\n\nv1.5 **shipped** with [receiver auth](https://x).\n\nSecond paragraph.\n<!-- waid:state:end -->` | `v1.5 shipped with receiver auth.` |
| markers with only the heading between them | `null` |
| a plain `## Current State` heading with no markers | `null` |

`shortAgo` (relative to now): 30s → `now`; 5min → `5m`; 11h → `11h`; 6d → `6d`; 8d → `1w`; `null` → `—`.

`feedCountText`: `{ total: 2, byStatus: { Open: 2 } }` → `2 open`; `{ total: 3, byStatus: { Open: 2, Closed: 1 } }` → `3 items`; `{ total: 1, byStatus: {} }` → `1 item`; kind `mind`, query `7d slug:whence` → `7d`; kind `mind`, no query → `14d`.

---

## 10. Verification

```bash
pnpm check                       # after every phase
cd src-tauri && cargo check      # after the capabilities edit: tauri-build rejects unknown permission ids
cd src-tauri && cargo test       # must be unchanged; no Rust was touched
pnpm tauri:wsl                   # manual script below (or `pnpm tauri dev` on Windows)
```

Manual script:

1. Dashboard, never entering widget mode: session start/end, link launch, webhook fire, Sync all, ⌘/Ctrl+K, ⌘/Ctrl+F all behave as before.
2. Click Widget mode. The window shrinks to the roster at top-right, stays above other windows, and shows the selected brief first and bold.
3. Click another app. After a beat the widget collapses to the rest line with the tally. Click the widget: the roster returns.
4. The lead's activity line matches its `## Activity` block minus the `last push` part. A brief never synced shows no line and no gap.
5. Type a note in the capture field, press Enter. It is appended under `## Captures` of the lead brief. Global `Ctrl+Shift+Space` from another app surfaces the widget with the capture field focused and no modal.
6. Open a non-lead row (accordion). Feeds load once; reopening the same row makes no network call. The brief's `last_opened` on disk has **not** changed.
7. From that leaf, click a link. The brief becomes the lead, the link opens, and (with a NeuroSkill connection) "· in session" appears. End session clears it.
8. Start an edit in the dashboard without saving, enter widget mode, return. The draft is intact.
9. Settings → Appearance → Widget detail → Side leaf. Enter widget mode near the right screen edge and open a row: the leaf appears to the left and the roster does not move. Drag the widget to the left half and open a row: the leaf appears to the right.
10. Move the widget, expand to the dashboard, re-enter: it returns to where it was left. Do this once with a left-opened side leaf open while moving.
11. Expand from a maximized dashboard start: after the round trip the dashboard is maximized again, the minimum size is back to 720×480, and it is no longer always-on-top.
12. Toggle light/dark and the accent in Settings, re-enter widget mode: colors follow.

---

## 11. Unverified, check these first

- **R1. Permission identifiers.** `core:window:allow-set-size` and `core:window:allow-set-always-on-top` are confirmed by Whence's working `capabilities/default.json`. `allow-set-min-size` and `allow-set-position` follow the same naming scheme but were not checked against a generated schema; `cargo check` will reject a wrong one. The getters (`innerSize`, `outerPosition`, `scaleFactor`, `currentMonitor`, `isMaximized`) and the `onFocusChanged` / `onMoved` listeners are believed to be covered by `core:default`. If one rejects at runtime, add its `core:window:allow-*` entry explicitly.
- **R2. Focus events under WSLg.** `onFocusChanged` for an undecorated always-on-top window has not been tried there. If it is unreliable, fall back to DOM `window` `focus` / `blur` events inside `WidgetShell`.
- **R3. Positioning under Wayland.** If the dev session is Wayland (possible under WSLg; not confirmed), `setPosition` is a no-op and `outerPosition` reads zeros. The `canPosition` flag in 5.5 handles this: default placement is left to the compositor, the side leaf always opens to the right, and position memory is skipped. Always-on-top may also be ignored there. The Windows build is the reference for all three.
- **R4. Side-leaf flicker.** Opening to the left needs a move and a resize, which are two calls. The `shifting` handshake hides the content between them. If a visible blink remains, try reversing the order (resize, then move) before considering anything heavier.
- **R5. Native shadow or border on Windows.** The widget reuses the dashboard's gutter, so it should look the same as the dashboard does today. If a rectangular outline is more noticeable at widget size, add `setShadow(false)` on enter and `setShadow(true)` on exit with `core:window:allow-set-shadow`.
- **R6. Toasts at rest height.** `Toasts` is `fixed bottom-[18px]`. In a ~100px rest window a toast will cover the lead row until it dismisses. Accepted for this version.
- **R7. macOS.** The traffic-light inset is a guess at the cheapest safe behavior. Nothing here was checked on macOS.

---

## 12. Open decisions, preserved

Each has a default this spec implements. None blocks the work.

1. **One window or two.** Default: one (3.1). A second always-on-top window would let the dashboard and widget coexist, at the cost of cross-realm sync for five stores.
2. **Hidden mount or unmount.** Default: hidden (3.2). Unmounting saves the three listener guards but loses edit drafts and needs a different session safety net.
3. **What "focus" means.** Default: window focus opens the roster, blur collapses it. Alternatives: hover; or a pin toggle that keeps the roster open when blurred.
4. **Remember widget mode across launches.** Default: no, the app always starts in the dashboard. Persisting it needs `visible: false` in `tauri.conf.json` and a show-after-layout step to avoid a 1100×720 flash, which touches startup for everyone.
5. **Who the lead is.** Default: `projects.selected`. Alternatives: the brief with an open session; or, once Whence ingestion lands, the project Whence reports as focused (slugs already match).
6. **One widget or two.** Whence's roster is keyed on the same slugs as WAID briefs. This leaf could hang off Whence's rows instead of living in a second always-on-top surface. Not designed here.
7. **Taskbar presence.** Default: untouched, so the window stays recoverable without a tray icon. Whence sets `skipTaskbar` and relies on its tray.
8. **macOS chrome.** Default: keep native traffic lights and inset the header. Alternative: turn decorations off in widget mode and restore the Overlay title bar on exit, once verified.

---

## 13. Out of scope

- Any change to how briefs are parsed, synced, synthesized, or stored.
- Polling, background refresh, or notifications.
- A tray icon, autostart, single-instance handling, or a global shortcut for widget mode.
- Arrow-key row navigation, drag-to-reorder in the roster, status editing from the widget.
- Mind State sync or LLM digest from the leaf.
- Reading Whence's timeline or live focus state.
