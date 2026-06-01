// Light/dark theme via Tailwind's `dark:` variant. We toggle a `dark` class on
// <html> and remember the choice in localStorage.

const KEY = "waid-theme";

function apply(dark: boolean) {
  document.documentElement.classList.toggle("dark", dark);
}

/** Resolve the initial theme (saved choice, else system preference) and apply it. */
export function initTheme(): boolean {
  const saved = localStorage.getItem(KEY);
  const dark = saved
    ? saved === "dark"
    : window.matchMedia("(prefers-color-scheme: dark)").matches;
  apply(dark);
  return dark;
}

export function setDark(dark: boolean): void {
  localStorage.setItem(KEY, dark ? "dark" : "light");
  apply(dark);
}

export function isDark(): boolean {
  return document.documentElement.classList.contains("dark");
}
