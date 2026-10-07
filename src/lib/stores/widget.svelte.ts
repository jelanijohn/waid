// Widget mode: a small always-on-top roster presentation of the same `main`
// window (see docs/spec-widget). The dashboard stays mounted but hidden while
// this is on; every store is shared, so nothing needs syncing across modes.

import { tick } from "svelte";
import type { Brief } from "$lib/types";
import { projects } from "$lib/stores/projects.svelte";
import { settings } from "$lib/stores/settings.svelte";
import {
  ROSTER_W,
  LEAF_W,
  LEAF_GAP,
  enterWidgetWindow,
  exitWidgetWindow,
  setWidgetSize,
  shiftWindowX,
  pickLeafSide,
  positionable,
  type SavedGeometry,
} from "$lib/widgetWindow";

export type WidgetMode = "dashboard" | "widget";

/** Blur → rest collapse grace, so a refocus inside the window cancels it. */
const BLUR_GRACE_MS = 200;

function isArchived(b: Brief): boolean {
  return (b.status ?? "").toLowerCase() === "archived";
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

class WidgetStore {
  mode = $state<WidgetMode>("dashboard");
  focused = $state(true);
  openPath = $state<string | null>(null);
  /** Meaningful for the side variant only. */
  leafSide = $state<"left" | "right">("left");
  captureDraft = $state("");
  captureFocusTick = $state(0);
  /** True while the window is being re-laid-out (enter, side-leaf open/close);
   *  the shell hides its content and skips its own resizes meanwhile. */
  shifting = $state(false);
  /** Whether the open leaf was opened as a side panel (fixed at open time, so
   *  flipping the setting mid-leaf still closes it the way it opened). */
  openAsSide = $state(false);
  /** Registered by WidgetShell on mount, cleared on destroy: the measured
   *  element's current content height in logical px. */
  measure: (() => number) | null = null;

  private saved: SavedGeometry | null = null;
  private blurTimer: ReturnType<typeof setTimeout> | null = null;
  private busy = false;

  /** The brief shown at rest and pinned first: the dashboard's selection,
   *  else the first non-archived brief. */
  get lead(): Brief | null {
    return projects.selected ?? projects.briefs.find((b) => !isArchived(b)) ?? null;
  }

  /** Lead first, then the rest in store order, archived dropped (unless it's
   *  the lead). Sidebar search / status filter don't apply. */
  get roster(): Brief[] {
    const lead = this.lead;
    if (!lead) return [];
    return [lead, ...projects.briefs.filter((b) => b.path !== lead.path && !isArchived(b))];
  }

  get openBrief(): Brief | null {
    if (!this.openPath) return null;
    return this.roster.find((b) => b.path === this.openPath) ?? null;
  }

  /** Whether a side leaf is currently showing. */
  get sideOpen(): boolean {
    return this.openAsSide && this.openBrief !== null;
  }

  /** Width of the measured content: the roster, plus the side leaf. */
  get contentW(): number {
    return ROSTER_W + (this.sideOpen ? LEAF_GAP + LEAF_W : 0);
  }

  /** Dashboard → widget. Mounts the (hidden) shell first so the very first
   *  resize already has the real roster height. */
  async enter(): Promise<void> {
    if (this.mode === "widget" || this.busy) return;
    this.busy = true;
    try {
      this.clearBlurTimer();
      this.openPath = null;
      this.openAsSide = false;
      this.focused = true;
      this.shifting = true;
      this.mode = "widget";
      await tick();
      const h = this.measure?.() ?? 0;
      this.saved = await enterWidgetWindow(settings.widgetPos, ROSTER_W, h);
      await nextFrame();
    } finally {
      this.shifting = false;
      this.busy = false;
    }
  }

  /** Widget → dashboard, optionally selecting a brief first ("Open brief"). */
  async exit(selectPath?: string): Promise<void> {
    if (this.mode !== "widget" || this.busy) return;
    this.busy = true;
    try {
      this.clearBlurTimer();
      this.shifting = true;
      this.openPath = null;
      this.openAsSide = false;
      if (selectPath && selectPath !== projects.selectedPath) await projects.select(selectPath);
      await exitWidgetWindow(this.saved);
      this.saved = null;
      this.mode = "dashboard";
    } finally {
      this.shifting = false;
      this.busy = false;
    }
  }

  /** Idempotent: fed by both the Tauri focus event and DOM focus/blur. Blur
   *  collapses to rest after a grace period, unless the document regained
   *  focus meanwhile. */
  setFocused(f: boolean): void {
    if (this.mode !== "widget") return;
    if (f) {
      this.clearBlurTimer();
      this.focused = true;
      return;
    }
    if (this.blurTimer || !this.focused) return;
    this.blurTimer = setTimeout(() => {
      this.blurTimer = null;
      if (this.mode !== "widget" || document.hasFocus()) return;
      this.focused = false;
      void this.closeLeaf();
    }, BLUR_GRACE_MS);
  }

  private clearBlurTimer(): void {
    if (this.blurTimer) clearTimeout(this.blurTimer);
    this.blurTimer = null;
  }

  async toggleLeaf(path: string): Promise<void> {
    if (this.shifting) return;
    if (this.openPath === path) return this.closeLeaf();
    // Switching rows (or the accordion) never changes the window's geometry
    // here; the shell's ResizeObserver follows the height.
    if (this.openPath !== null || settings.widgetLeaf !== "side") {
      if (this.openPath === null) this.openAsSide = false;
      this.openPath = path;
      return;
    }

    // Side leaf, opening. To the left: move the window left by the leaf's
    // width and widen it, so the roster stays put on screen. To the right:
    // only widen. Content stays hidden until both have landed.
    this.shifting = true;
    try {
      const side = positionable() ? await pickLeafSide() : "right";
      this.leafSide = side;
      this.openAsSide = true;
      this.openPath = path;
      await tick();
      const h = this.measure?.() ?? 0;
      const fullW = ROSTER_W + LEAF_GAP + LEAF_W;
      if (side === "left") {
        const [moved] = await Promise.all([shiftWindowX(-(LEAF_W + LEAF_GAP)), setWidgetSize(fullW, h)]);
        // Couldn't move: put the leaf on the right so the roster doesn't jump.
        if (!moved) this.leafSide = "right";
      } else {
        await setWidgetSize(fullW, h);
      }
      await tick();
      await nextFrame();
    } finally {
      this.shifting = false;
    }
  }

  async closeLeaf(): Promise<void> {
    if (this.openPath === null) return;
    if (!this.openAsSide) {
      this.openPath = null;
      return;
    }
    // Side leaf, closing: shrink and (if it was on the left) move back right.
    this.shifting = true;
    try {
      const wasLeft = this.leafSide === "left";
      this.openPath = null;
      this.openAsSide = false;
      await tick();
      const h = this.measure?.() ?? 0;
      if (wasLeft) {
        await Promise.all([setWidgetSize(ROSTER_W, h), shiftWindowX(LEAF_W + LEAF_GAP)]);
      } else {
        await setWidgetSize(ROSTER_W, h);
      }
      await nextFrame();
    } finally {
      this.shifting = false;
    }
  }

  /** Quick-capture shortcut in widget mode: show the roster and focus the
   *  capture field instead of opening the modal. */
  requestCaptureFocus(): void {
    if (this.mode !== "widget") return;
    this.clearBlurTimer();
    this.focused = true;
    this.captureFocusTick++;
  }
}

export const widget = new WidgetStore();
