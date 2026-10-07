// Every window-API call widget mode makes, in one place. Widget mode re-flags
// and resizes the existing `main` window (no second window, see the widget
// spec §3.1). Like Titlebar / ResizeHandles, these talk to
// @tauri-apps/api/window directly; lib/tauri.ts stays backend-commands-only.
// Every function swallows errors so plain `vite dev` (no Tauri) just no-ops.

import {
  getCurrentWindow,
  currentMonitor,
  monitorFromPoint,
  LogicalSize,
  LogicalPosition,
  PhysicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";
import { platform, paintsOwnFrame } from "$lib/platform";

export const ROSTER_W = 300;
export const LEAF_W = 340;
/** Transparent gutter around the panel(s); matches +page.svelte's p-[14px].
 *  Zero on Windows, where the OS draws the frame (see lib/platform.ts). */
export const GUTTER = paintsOwnFrame ? 14 : 0;
/** Gap between roster and side leaf. Zero on Windows: the compositor draws one
 *  rounded rect around the whole window, so the leaf is a divided column. */
export const LEAF_GAP = paintsOwnFrame ? 10 : 0;
export const EDGE_MARGIN = 24;
/** Smallest content height the widget asks for (header + one row, roughly). */
const MIN_CONTENT_H = 60;
/** tauri.conf.json minWidth / minHeight, restored on exit. */
const DASH_MIN = { w: 720, h: 480 };

/** Dashboard geometry to restore on exit, in logical px. `x`/`y` are the
 *  outer position (what setPosition sets), `w`/`h` the inner size (what
 *  setSize sets). */
export interface SavedGeometry {
  x: number;
  y: number;
  w: number;
  h: number;
  maximized: boolean;
}

/** False once a move didn't take (Wayland ignores setPosition and reads back
 *  zeros). Then the side leaf always opens right, nothing shifts, position
 *  memory is skipped, and exit doesn't try to restore the position. */
let canPosition = true;
export function positionable(): boolean {
  return canPosition;
}

/** Tauri's monitor getters are unsafe on Linux: tauri-runtime-wry fetches the
 *  gdk::Monitor on the main thread but reads its geometry (GDK → Xlib) on the
 *  IPC thread, which corrupts the X connection — the next window call crashes
 *  ("xcb_xlib_threads_sequence_lost") or hangs. There we read the screen from
 *  the DOM instead and skip the off-screen anchor check. */
const nativeMonitors = platform !== "linux";

/** Work area of the window's monitor, in physical px, or null. */
async function workArea(): Promise<{ x: number; y: number; w: number; h: number } | null> {
  if (nativeMonitors) {
    const m = await currentMonitor();
    if (!m) return null;
    const wa = m.workArea;
    return { x: wa.position.x, y: wa.position.y, w: wa.size.width, h: wa.size.height };
  }
  const s = window.screen as Screen & { availLeft?: number; availTop?: number };
  if (!s?.availWidth || !s.availHeight) return null;
  const sf = await getCurrentWindow().scaleFactor();
  return {
    x: (s.availLeft ?? 0) * sf,
    y: (s.availTop ?? 0) * sf,
    w: s.availWidth * sf,
    h: s.availHeight * sf,
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Run one window call, ignoring failure, so a later step still runs. */
async function attempt(fn: () => Promise<unknown>): Promise<void> {
  try {
    await fn();
  } catch {
    // Window API unavailable or the call was refused.
  }
}

/** Move to a physical position and confirm it took (poll the read-back, since
 *  some backends apply the move asynchronously). A move that never lands marks
 *  the platform as unpositionable for the rest of this widget session. */
async function moveToPhysical(x: number, y: number): Promise<boolean> {
  try {
    const win = getCurrentWindow();
    const sf = await win.scaleFactor();
    const tol = Math.ceil(sf) + 1;
    const tx = Math.round(x);
    const ty = Math.round(y);
    await win.setPosition(new PhysicalPosition(tx, ty));
    for (let i = 0; i < 6; i++) {
      const p = await win.outerPosition();
      if (Math.abs(p.x - tx) <= tol && Math.abs(p.y - ty) <= tol) return true;
      await sleep(50);
    }
  } catch {
    // fall through
  }
  canPosition = false;
  return false;
}

// setWidgetSize bookkeeping: requests are serialized through one promise
// chain and deduped against the last *requested* physical size (never a
// read-back — fractional DPI rounding would make that loop).
let sizeChain: Promise<void> = Promise.resolve();
let lastRequested: string | null = null;
let lastContent: { w: number; h: number } | null = null;

/** Size the window to hold `contentW × contentH` logical px of content (adds
 *  the gutter itself). Rounds up in physical px so a 1.25× display loses a
 *  sliver of gutter rather than clipping the last row. */
export function setWidgetSize(contentW: number, contentH: number): Promise<void> {
  lastContent = { w: contentW, h: contentH };
  sizeChain = sizeChain.then(async () => {
    try {
      const win = getCurrentWindow();
      const sf = await win.scaleFactor();
      const w = Math.ceil((contentW + 2 * GUTTER) * sf);
      const h = Math.ceil((Math.max(contentH, MIN_CONTENT_H) + 2 * GUTTER) * sf);
      const key = `${w}x${h}`;
      if (key === lastRequested) return;
      await win.setSize(new PhysicalSize(w, h));
      // Only after success, so a refused resize is retried next time.
      lastRequested = key;
    } catch {
      // Window API unavailable.
    }
  });
  return sizeChain;
}

/** Re-send the last content size (after a scale-factor change). */
export function reapplyWidgetSize(): Promise<void> {
  lastRequested = null;
  return lastContent ? setWidgetSize(lastContent.w, lastContent.h) : Promise.resolve();
}

/** Enter widget geometry: un-maximize, remember the dashboard rect, drop the
 *  minimum, go always-on-top, size to the already-measured content, place it.
 *  Returns the geometry to restore later (null if it couldn't be read). */
export async function enterWidgetWindow(
  anchor: { x: number; y: number } | null,
  contentW: number,
  contentH: number,
): Promise<SavedGeometry | null> {
  canPosition = true;
  lastRequested = null;
  let saved: SavedGeometry | null = null;
  let win;
  try {
    win = getCurrentWindow();
  } catch {
    return null;
  }

  try {
    // Never resize while maximized (Windows keeps WS_MAXIMIZE; GTK defers),
    // and read the geometry only after un-maximizing so we save the real
    // restore rect. GTK un-maximizes asynchronously, hence the poll.
    const maximized = await win.isMaximized();
    if (maximized) {
      await win.toggleMaximize();
      for (let i = 0; i < 6 && (await win.isMaximized()); i++) await sleep(50);
    }
    const sf = await win.scaleFactor();
    const size = (await win.innerSize()).toLogical(sf);
    const pos = (await win.outerPosition()).toLogical(sf);
    saved = { x: pos.x, y: pos.y, w: size.width, h: size.height, maximized };
  } catch {
    // Keep going: the widget still works, exit just can't restore the rect.
  }

  // The minimum must drop before the shrink, or 720×480 clamps it.
  await attempt(() =>
    win.setMinSize(new LogicalSize(ROSTER_W + 2 * GUTTER, MIN_CONTENT_H + 2 * GUTTER)),
  );
  await attempt(() => win.setAlwaysOnTop(true));
  await setWidgetSize(contentW, contentH);

  // Placement: the remembered roster anchor if it's still on a monitor, else
  // top-right of the current monitor's work area.
  try {
    const sf = await win.scaleFactor();
    let target: { x: number; y: number } | null = null;
    const winW = Math.ceil((contentW + 2 * GUTTER) * sf);
    if (anchor && nativeMonitors) {
      if (await monitorFromPoint(anchor.x * sf, anchor.y * sf)) {
        target = { x: anchor.x * sf, y: anchor.y * sf };
      }
    } else if (anchor) {
      // No monitor lookup on Linux: keep the anchor inside the screen instead.
      const wa = await workArea();
      target = wa
        ? {
            x: Math.min(Math.max(anchor.x * sf, wa.x), wa.x + wa.w - winW),
            y: Math.min(Math.max(anchor.y * sf, wa.y), wa.y + wa.h - MIN_CONTENT_H * sf),
          }
        : { x: anchor.x * sf, y: anchor.y * sf };
    }
    if (!target) {
      const wa = await workArea();
      if (wa) target = { x: wa.x + wa.w - winW - EDGE_MARGIN * sf, y: wa.y + EDGE_MARGIN * sf };
    }
    if (target) await moveToPhysical(target.x, target.y);
  } catch {
    canPosition = false;
  }
  return saved;
}

/** Restore the dashboard: always-on-top off, size, position, the 720×480
 *  minimum (last, to avoid GTK's double jump), then re-maximize if it was. */
export async function exitWidgetWindow(saved: SavedGeometry | null): Promise<void> {
  let win;
  try {
    win = getCurrentWindow();
  } catch {
    return;
  }
  lastRequested = null;
  lastContent = null;
  await attempt(() => win.setAlwaysOnTop(false));
  if (saved) {
    await attempt(() => win.setSize(new LogicalSize(saved.w, saved.h)));
    if (canPosition) await attempt(() => win.setPosition(new LogicalPosition(saved.x, saved.y)));
  }
  await attempt(() => win.setMinSize(new LogicalSize(DASH_MIN.w, DASH_MIN.h)));
  if (saved?.maximized) await attempt(() => win.toggleMaximize());
}

/** Move the window horizontally by `dx` logical px; false if it didn't move. */
export async function shiftWindowX(dx: number): Promise<boolean> {
  if (!canPosition) return false;
  try {
    const win = getCurrentWindow();
    const sf = await win.scaleFactor();
    const p = await win.outerPosition();
    return await moveToPhysical(p.x + Math.round(dx * sf), p.y);
  } catch {
    return false;
  }
}

/** "left" when the roster's centre is in the right half of its monitor and
 *  there's room for the leaf on that side, else "right". */
export async function pickLeafSide(): Promise<"left" | "right"> {
  if (!canPosition) return "right";
  try {
    const win = getCurrentWindow();
    const wa = await workArea();
    if (!wa) return "right";
    const sf = await win.scaleFactor();
    const p = await win.outerPosition();
    const s = await win.outerSize();
    const centre = p.x + s.width / 2;
    const roomLeft = p.x - (LEAF_W + LEAF_GAP) * sf >= wa.x;
    return centre > wa.x + wa.w / 2 && roomLeft ? "left" : "right";
  } catch {
    return "right";
  }
}

/** Current window top-left in logical px, or null (also null when the
 *  platform can't position windows, so nothing gets remembered). */
export async function windowPosition(): Promise<{ x: number; y: number } | null> {
  if (!canPosition) return null;
  try {
    const win = getCurrentWindow();
    const sf = await win.scaleFactor();
    const p = (await win.outerPosition()).toLogical(sf);
    return { x: p.x, y: p.y };
  } catch {
    return null;
  }
}
