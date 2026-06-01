// Central project (brief) state, Svelte 5 runes flavour.

import type { Brief } from "$lib/types";
import {
  listBriefs,
  getBriefsDir,
  touchBrief,
  saveBrief as saveBriefCmd,
} from "$lib/tauri";

class ProjectStore {
  briefs = $state<Brief[]>([]);
  selectedPath = $state<string | null>(null);
  briefsDir = $state<string>("");
  loading = $state(false);
  error = $state<string | null>(null);

  /** The currently selected brief, or null. */
  get selected(): Brief | null {
    return this.briefs.find((b) => b.path === this.selectedPath) ?? null;
  }

  /** (Re)load all briefs from disk. Keeps the current selection if it survives. */
  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      this.briefsDir = await getBriefsDir();
      const briefs = await listBriefs();
      this.briefs = briefs;
      if (
        this.selectedPath &&
        !briefs.some((b) => b.path === this.selectedPath)
      ) {
        this.selectedPath = null;
      }
      if (!this.selectedPath && briefs.length > 0) {
        await this.select(briefs[0].path);
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  /** Select a brief and stamp its `last_opened` on disk. */
  async select(path: string): Promise<void> {
    this.selectedPath = path;
    try {
      const updated = await touchBrief(path);
      this.upsert(updated);
    } catch (e) {
      // Touch failing shouldn't block selection.
      console.error("touch_brief failed:", e);
    }
  }

  /** Save edited raw content back to disk and refresh that brief in place. */
  async save(path: string, content: string): Promise<Brief> {
    const updated = await saveBriefCmd(path, content);
    this.upsert(updated);
    return updated;
  }

  /** Replace (or insert) a brief by path, without reordering the list. */
  upsert(brief: Brief): void {
    const idx = this.briefs.findIndex((b) => b.path === brief.path);
    if (idx === -1) {
      this.briefs = [...this.briefs, brief];
    } else {
      const next = this.briefs.slice();
      next[idx] = brief;
      this.briefs = next;
    }
  }
}

export const projects = new ProjectStore();
