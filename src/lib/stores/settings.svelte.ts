// User preferences: theme + the Tidewater appearance tweaks (accent, sidebar
// card style, density). Persisted to localStorage and applied to the DOM. The
// `dark` class lives on <html> (Tailwind's dark variant); accent + density are
// applied on the app shell in +page.svelte and read reactively by components.

export type SidebarStyle = "rows" | "compact" | "rocks";
export type Density = "comfortable" | "compact";

/** Curated accent options exposed in settings (blue, green, indigo, coral). */
export const ACCENTS = ["#1E88E5", "#40A87E", "#6366F1", "#E57373"] as const;

const KEY = "waid-settings";
const LEGACY_THEME_KEY = "waid-theme"; // pre-Tidewater: just "dark"/"light"

interface Persisted {
  dark: boolean;
  accent: string;
  sidebarStyle: SidebarStyle;
  density: Density;
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
  density = $state<Density>("comfortable");

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
    this.density = saved.density ?? "comfortable";
    applyDark(this.dark);
    applyAccent(this.accent);
  }

  private persist(): void {
    const data: Persisted = {
      dark: this.dark,
      accent: this.accent,
      sidebarStyle: this.sidebarStyle,
      density: this.density,
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

  setDensity(density: Density): void {
    this.density = density;
    this.persist();
  }
}

export const settings = new Settings();
