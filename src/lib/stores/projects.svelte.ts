// Central project (brief) state, Svelte 5 runes flavour.

import type { Brief, SyncOutcome, VaultInfo } from "$lib/types";
import {
  listBriefs,
  getBriefsDir,
  getVaultInfo,
  isWsl as isWslCmd,
  touchBrief,
  saveBrief as saveBriefCmd,
  syncBrief as syncBriefCmd,
  syncAll as syncAllCmd,
  syncMindState as syncMindStateCmd,
  markBriefSession as markBriefSessionCmd,
  synthesizeBrief as synthesizeBriefCmd,
  synthesizeAll as synthesizeAllCmd,
  getLlmSettings,
} from "$lib/tauri";
import { settings } from "$lib/stores/settings.svelte";

/** Normalise a wikilink target / note name for case-insensitive matching. */
function normalizeTarget(target: string): string {
  // Drop any `#heading` / `|alias` and surrounding whitespace, then lowercase.
  return target.split(/[#|]/)[0].trim().toLowerCase();
}

/** Mirrors the backend's parse_github_url: a github.com/{owner}/{repo} link. */
const GITHUB_URL = /^https?:\/\/(www\.)?github\.com\/[^/]+\/[^/]+/i;

/** Whether a brief has anything to sync (a GitHub link or an explicit source). */
export function isSyncableBrief(brief: Brief): boolean {
  return (
    brief.links.some((l) => GITHUB_URL.test(l.url)) ||
    (brief.sources?.some((s) => s.url.trim().length > 0) ?? false)
  );
}

/** The first github.com/{owner}/{repo} URL among a brief's links, or null.
 *  Single source of truth for the GITHUB_URL regex (used by the bootstrap
 *  chooser to recommend the GitHub method). */
export function githubUrlInBrief(brief: Brief): string | null {
  return brief.links.find((l) => GITHUB_URL.test(l.url))?.url ?? null;
}

/** A brief's filename without the `.md` extension (its Obsidian note name). */
function stem(brief: Brief): string {
  return brief.fileName.replace(/\.md$/i, "");
}

class ProjectStore {
  briefs = $state<Brief[]>([]);
  selectedPath = $state<string | null>(null);
  /** Path of a just-created brief that should auto-open the Generate modal on
   *  its next mount. One-shot: ProjectDetail reads it once and clears it. */
  bootstrapPath = $state<string | null>(null);
  briefsDir = $state<string>("");
  vault = $state<VaultInfo>({ isVault: false });
  /** True under WSL — used to add a handler hint when opening URLs fails. */
  isWsl = $state(false);
  /** Configured LLM synthesis provider ("ollama" | "anthropic"), or null when
   *  synthesis is disabled. Drives whether Refresh also runs synthesis. */
  llmProvider = $state<string | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  /** Sidebar search query (matched against name/description/tags/body). */
  query = $state("");
  /** Active status filter, or null for "all". Archived is hidden unless picked. */
  statusFilter = $state<string | null>(null);

  /** The currently selected brief, or null. */
  get selected(): Brief | null {
    return this.briefs.find((b) => b.path === this.selectedPath) ?? null;
  }

  /** Distinct statuses present in the data, for the filter pills. Known
   *  statuses come first in a fixed order; any custom ones follow, sorted. */
  get statuses(): string[] {
    const rank = (s: string) =>
      ["active", "paused", "blocked", "archived"].indexOf(s.toLowerCase());
    const byLower = new Map<string, string>(); // lowercased → original spelling
    for (const b of this.briefs) {
      const s = (b.status ?? "").trim();
      if (s && !byLower.has(s.toLowerCase())) byLower.set(s.toLowerCase(), s);
    }
    return [...byLower.values()].sort((a, b) => {
      const ra = rank(a), rb = rank(b);
      if (ra !== rb) return (ra === -1 ? 99 : ra) - (rb === -1 ? 99 : rb);
      return a.localeCompare(b);
    });
  }

  /** Briefs shown in the sidebar after applying search + status filter.
   *  With no status filter, archived briefs are hidden; selecting the
   *  "archived" pill is what surfaces them (the archive view). */
  get filtered(): Brief[] {
    const q = this.query.trim().toLowerCase();
    const status = this.statusFilter?.toLowerCase() ?? null;
    return this.briefs.filter((b) => {
      const s = (b.status ?? "").toLowerCase();
      if (status) {
        if (s !== status) return false;
      } else if (s === "archived") {
        return false;
      }
      if (!q) return true;
      const hay = [b.name, b.description ?? "", b.tags.join(" "), b.body]
        .join("\n")
        .toLowerCase();
      return hay.includes(q);
    });
  }

  /** Index from normalised note name → brief, for resolving `[[wikilinks]]`. */
  get nameIndex(): Map<string, Brief> {
    const map = new Map<string, Brief>();
    for (const b of this.briefs) {
      // Filename stem is Obsidian's primary key; fall back to display name.
      // First writer wins so the index is stable regardless of load order.
      for (const key of [stem(b), b.name]) {
        const norm = key.trim().toLowerCase();
        if (norm && !map.has(norm)) map.set(norm, b);
      }
    }
    return map;
  }

  /** Resolve a wikilink target to a brief, or null if it points nowhere. */
  resolveWikilink(target: string): Brief | null {
    return this.nameIndex.get(normalizeTarget(target)) ?? null;
  }

  /** Build an `obsidian://open` deep link for a brief, or null when not in a
   *  vault (or the brief somehow sits outside the resolved vault root). */
  obsidianUri(brief: Brief): string | null {
    const { isVault, name, root } = this.vault;
    if (!isVault || !name || !root) return null;
    const sep = brief.path.includes("\\") ? "\\" : "/";
    const prefix = root.endsWith(sep) ? root : root + sep;
    if (!brief.path.startsWith(prefix)) return null;
    // Vault-relative path, POSIX separators, no extension — Obsidian's `file`.
    const rel = brief.path.slice(prefix.length).replace(/\\/g, "/").replace(/\.md$/i, "");
    return `obsidian://open?vault=${encodeURIComponent(name)}&file=${encodeURIComponent(rel)}`;
  }

  /** Other briefs whose body wikilinks to the given one ("Linked from"). */
  backlinksFor(path: string): Brief[] {
    const target = this.briefs.find((b) => b.path === path);
    if (!target) return [];
    const keys = new Set([stem(target).toLowerCase(), target.name.toLowerCase()]);
    const wikilink = /\[\[([^\]\n]+?)\]\]/g;
    return this.briefs.filter((b) => {
      if (b.path === path) return false;
      let m: RegExpExecArray | null;
      wikilink.lastIndex = 0;
      while ((m = wikilink.exec(b.body))) {
        if (keys.has(normalizeTarget(m[1]))) return true;
      }
      return false;
    });
  }

  /** (Re)load all briefs from disk. Keeps the current selection if it survives. */
  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      this.briefsDir = await getBriefsDir();
      this.vault = await getVaultInfo();
      this.isWsl = await isWslCmd();
      await this.refreshLlmProvider();
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
    // Opt-in: auto-refresh the brief's sync block on open (debounced).
    this.maybeAutoSync(path);
  }

  /** Save edited raw content back to disk and refresh that brief in place. */
  async save(path: string, content: string): Promise<Brief> {
    const updated = await saveBriefCmd(path, content);
    this.upsert(updated);
    return updated;
  }

  /** Refresh one brief's managed sync block from its integrations. */
  async sync(path: string): Promise<Brief> {
    const updated = await syncBriefCmd(path);
    this.upsert(updated);
    return updated;
  }

  /** Synthesize one brief's Current State + Open Questions via the LLM. */
  async synthesize(path: string): Promise<Brief> {
    const updated = await synthesizeBriefCmd(path);
    this.upsert(updated);
    return updated;
  }

  /** Regenerate one brief's `## Mind State` region from NeuroSkill data. */
  async syncMind(path: string): Promise<Brief> {
    const updated = await syncMindStateCmd(path);
    this.upsert(updated);
    return updated;
  }

  /** Fire a NeuroSkill session label (best-effort; never throws, so launch /
   *  navigation is never blocked). Returns `false` when the label could not be
   *  fired — e.g. the NeuroSkill daemon is unreachable — so the caller can
   *  surface it; `true` on success (including the no-op for briefs without a
   *  NeuroSkill connection, which the backend reports as success). */
  async markSession(path: string, phase: "start" | "end"): Promise<boolean> {
    try {
      await markBriefSessionCmd(path, phase);
      return true;
    } catch (e) {
      // The daemon may be down / unreachable — let the caller decide whether
      // to warn the user; never block the launch on it.
      console.info("mark_brief_session:", e);
      return false;
    }
  }

  /** Re-read the configured synthesis provider from settings (null if off).
   *  Called on load and after the settings UI changes the provider. */
  async refreshLlmProvider(): Promise<void> {
    try {
      this.llmProvider = (await getLlmSettings()).llmProvider ?? null;
    } catch {
      this.llmProvider = null;
    }
  }

  /** Sync every syncable brief; reloads the list to pick up new bodies. */
  async syncAll(): Promise<SyncOutcome[]> {
    const outcomes = await syncAllCmd();
    await this.load();
    return outcomes;
  }

  /** Synthesize every syncable brief; reloads the list to pick up new bodies. */
  async synthesizeAll(): Promise<SyncOutcome[]> {
    const outcomes = await synthesizeAllCmd();
    await this.load();
    return outcomes;
  }

  /** Debounce timer for auto-sync-on-open (Phase 4). */
  private autoSyncTimer: ReturnType<typeof setTimeout> | null = null;

  /** When auto-sync is enabled, sync the just-opened brief after a short delay,
   *  but only if it's still the selection (rapid switching shouldn't spam the
   *  API). No-op when the setting is off or the brief has no sources. */
  private maybeAutoSync(path: string): void {
    if (this.autoSyncTimer) clearTimeout(this.autoSyncTimer);
    if (!settings.autoSyncOnOpen) return;
    const brief = this.briefs.find((b) => b.path === path);
    if (!brief || !isSyncableBrief(brief)) return;
    this.autoSyncTimer = setTimeout(() => {
      if (this.selectedPath !== path) return; // moved on — skip
      this.sync(path).catch((e) => console.error("auto-sync failed:", e));
    }, 800);
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
