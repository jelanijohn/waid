// Which `##` sections of a brief body the user has collapsed, keyed by brief
// path. Persisted to localStorage so the state survives re-renders (a sync or
// capture rewrites the body, which replaces the `{@html}` DOM) and restarts.
// Only collapsed sections are stored — the default for any section is open.

const KEY = "waid-collapsed-sections";

type Store = Record<string, string[]>;

function read(): Store {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Store) : {};
  } catch {
    return {};
  }
}

function write(store: Store): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(store));
  } catch {
    // Storage unavailable — collapse state just won't persist.
  }
}

/** The collapsed section keys (see `sectionize`) for one brief. */
export function collapsedSections(briefPath: string): Set<string> {
  return new Set(read()[briefPath] ?? []);
}

export function setSectionCollapsed(briefPath: string, section: string, collapsed: boolean): void {
  const store = read();
  const set = new Set(store[briefPath] ?? []);
  if (collapsed) set.add(section);
  else set.delete(section);
  if (set.size) store[briefPath] = [...set];
  else delete store[briefPath];
  write(store);
}
