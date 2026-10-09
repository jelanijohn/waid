// User preferences: theme + the Tidewater appearance tweaks (accent, sidebar
// card style, density). Persisted to localStorage and applied to the DOM. The
// `dark` class lives on <html> (Tailwind's dark variant); accent + density are
// applied on the app shell in +page.svelte and read reactively by components.

export type SidebarStyle = "rows" | "compact" | "rocks";
export type Density = "comfortable" | "compact";
/** Where a brief's live state sits relative to its body (see ProjectDetail). */
export type BriefLayout = "two-col" | "body" | "quiet";
/** Where a widget-mode leaf opens: inline under its row, or beside the roster. */
export type WidgetLeaf = "accordion" | "side";

/** Curated accent options exposed in settings (blue, green, indigo, coral). */
export const ACCENTS = ["#1E88E5", "#40A87E", "#6366F1", "#E57373"] as const;

/** Two-column live-state rail width in px. The upper bound (50% of the row)
 *  depends on the live layout, so ProjectDetail enforces it. */
export const RAIL_WIDTH_DEFAULT = 320;
export const RAIL_WIDTH_MIN = 280;

/** Widget-mode opacity floor: the slider's minimum and the clamp. Below this
 *  the roster goes unreadable and effectively un-clickable. */
export const WIDGET_OPACITY_MIN = 0.3;

/** Clamp any value into the legal `[WIDGET_OPACITY_MIN, 1]` range; anything
 *  non-numeric reads as fully opaque. */
export function clampWidgetOpacity(v: unknown): number {
  const n = typeof v === "number" && Number.isFinite(v) ? v : 1;
  return Math.min(1, Math.max(WIDGET_OPACITY_MIN, n));
}

const KEY = "waid-settings";
const LEGACY_THEME_KEY = "waid-theme"; // pre-Tidewater: just "dark"/"light"

interface Persisted {
  dark: boolean;
  accent: string;
  sidebarStyle: SidebarStyle;
  /** Alternate-row shading in the rows/compact sidebar lists. */
  sidebarZebra: boolean;
  briefLayout: BriefLayout;
  density: Density;
  autoSyncOnOpen: boolean;
  railWidth: number;
  widgetLeaf: WidgetLeaf;
  /** Widget mode: whole-widget opacity, `WIDGET_OPACITY_MIN..1` (1 = opaque).
   *  A constant, user-set value — never adaptive — applied as CSS `opacity`
   *  on the widget panels; the dashboard is unaffected. */
  widgetOpacity: number;
  /** Widget mode: where the roster's top-left was last left (logical px). */
  widgetPos: { x: number; y: number } | null;
}

/** A saved widget position, or null unless it's two finite numbers. */
function validPos(v: unknown): { x: number; y: number } | null {
  if (!v || typeof v !== "object") return null;
  const { x, y } = v as { x?: unknown; y?: unknown };
  return typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y)
    ? { x, y }
    : null;
}

function applyDark(dark: boolean): void {
  document.documentElement.classList.toggle("dark", dark);
}

/** Accent is set on <html> so it reaches every tree (modal/toasts included),
 *  not just the app shell; derived tokens resolve it lazily. */
function applyAccent(accent: string): void {
  document.documentElement.style.setProperty("--accent", accent);
}

class Settings {
  dark = $state(false);
  accent = $state<string>(ACCENTS[0]);
  sidebarStyle = $state<SidebarStyle>("rows");
  sidebarZebra = $state(false);
  briefLayout = $state<BriefLayout>("two-col");
  density = $state<Density>("comfortable");
  /** Opt-in: auto-refresh a brief's sync block when it's opened. Off by
   *  default — sync is manual unless the user turns this on. */
  autoSyncOnOpen = $state(false);
  railWidth = $state(RAIL_WIDTH_DEFAULT);
  widgetLeaf = $state<WidgetLeaf>("accordion");
  widgetOpacity = $state(1);
  widgetPos = $state<{ x: number; y: number } | null>(null);

  /** Resolve saved prefs (or sensible defaults) and apply the dark class. */
  init(): void {
    let saved: Partial<Persisted> = {};
    try {
      saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    } catch {
      // Corrupt blob — fall back to defaults.
    }

    const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const legacy = localStorage.getItem(LEGACY_THEME_KEY);
    const legacyDark = legacy ? legacy === "dark" : null;

    this.dark = saved.dark ?? legacyDark ?? systemDark;
    this.accent = saved.accent ?? ACCENTS[0];
    this.sidebarStyle = saved.sidebarStyle ?? "rows";
    this.sidebarZebra = saved.sidebarZebra ?? false;
    this.briefLayout = saved.briefLayout ?? "two-col";
    this.density = saved.density ?? "comfortable";
    this.autoSyncOnOpen = saved.autoSyncOnOpen ?? false;
    this.railWidth =
      typeof saved.railWidth === "number" && Number.isFinite(saved.railWidth)
        ? Math.max(RAIL_WIDTH_MIN, Math.round(saved.railWidth))
        : RAIL_WIDTH_DEFAULT;
    this.widgetLeaf = saved.widgetLeaf === "side" ? "side" : "accordion";
    this.widgetOpacity = clampWidgetOpacity(saved.widgetOpacity);
    this.widgetPos = validPos(saved.widgetPos);
    applyDark(this.dark);
    applyAccent(this.accent);
  }

  private persist(): void {
    const data: Persisted = {
      dark: this.dark,
      accent: this.accent,
      sidebarStyle: this.sidebarStyle,
      sidebarZebra: this.sidebarZebra,
      briefLayout: this.briefLayout,
      density: this.density,
      autoSyncOnOpen: this.autoSyncOnOpen,
      railWidth: this.railWidth,
      widgetLeaf: this.widgetLeaf,
      widgetOpacity: this.widgetOpacity,
      widgetPos: this.widgetPos,
    };
    localStorage.setItem(KEY, JSON.stringify(data));
  }

  setDark(dark: boolean): void {
    this.dark = dark;
    applyDark(dark);
    this.persist();
  }

  toggleDark(): void {
    this.setDark(!this.dark);
  }

  setAccent(accent: string): void {
    this.accent = accent;
    applyAccent(accent);
    this.persist();
  }

  setSidebarStyle(style: SidebarStyle): void {
    this.sidebarStyle = style;
    this.persist();
  }

  setSidebarZebra(on: boolean): void {
    this.sidebarZebra = on;
    this.persist();
  }

  setBriefLayout(layout: BriefLayout): void {
    this.briefLayout = layout;
    this.persist();
  }

  setDensity(density: Density): void {
    this.density = density;
    this.persist();
  }

  setAutoSyncOnOpen(on: boolean): void {
    this.autoSyncOnOpen = on;
    this.persist();
  }

  /** `save = false` while dragging — state updates live, localStorage is
   *  written once on release. */
  setRailWidth(px: number, save = true): void {
    this.railWidth = Math.max(RAIL_WIDTH_MIN, Math.round(px));
    if (save) this.persist();
  }

  setWidgetLeaf(leaf: WidgetLeaf): void {
    this.widgetLeaf = leaf;
    this.persist();
  }

  /** `save = false` while dragging the slider — state updates live (the
   *  Settings preview follows it; the widget itself can't be on screen while
   *  Settings is open), localStorage is written once on release. */
  setWidgetOpacity(v: number, save = true): void {
    this.widgetOpacity = clampWidgetOpacity(v);
    if (save) this.persist();
  }

  setWidgetPos(pos: { x: number; y: number } | null): void {
    this.widgetPos = validPos(pos);
    this.persist();
  }
}

export const settings = new Settings();
