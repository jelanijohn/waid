/** Which desktop OS the webview is running on. Sniffed from the webview's
 *  userAgent so we need no extra Tauri plugin: WKWebView → "Macintosh",
 *  WebView2 → "Windows", WebKitGTK on Linux/WSL → anything else. */
export type Platform = "macos" | "windows" | "linux";

export function detectPlatform(): Platform {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Mac OS X|Macintosh/.test(ua)) return "macos";
  if (/Windows/.test(ua)) return "windows";
  return "linux";
}

export const platform: Platform = detectPlatform();

/** Window-shape strategy. Windows 11's compositor rounds, borders and shadows
 *  the undecorated window for us (Tauri's `shadow` option, on by default), so
 *  there the page fills the window edge-to-edge and the OS draws the corners —
 *  a CSS radius would just stack a second, mismatched rounding inside the OS
 *  one. Everywhere else the page paints its own rounded panel inside a 14px
 *  transparent gutter: WebKitGTK on Linux/WSL has no compositor doing it, and
 *  macOS (native Overlay title bar) is deliberately left as it was. */
export const paintsOwnFrame = platform !== "windows";
