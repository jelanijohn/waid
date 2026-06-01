import { marked, type Tokens } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * Resolver for the current render pass: given a wikilink target, return whether
 * it points at a known brief. Set per-call by `renderMarkdown` so the extension
 * (registered once, below) can style resolved vs. missing links.
 */
let resolveExists: ((target: string) => boolean) | null = null;

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface WikilinkToken extends Tokens.Generic {
  type: "wikilink";
  target: string;
  alias: string;
}

// Obsidian-style `[[target]]`, `[[target|alias]]`, `[[target#heading]]` and the
// embed form `![[…]]`. Registered as an inline extension so marked still carves
// out code spans first — `[[x]]` inside backticks is left untouched.
const wikilinkExtension = {
  name: "wikilink",
  level: "inline" as const,
  start(src: string) {
    const i = src.indexOf("[[");
    const j = src.indexOf("![[");
    if (i === -1) return j === -1 ? undefined : j;
    if (j === -1) return i;
    return Math.min(i, j);
  },
  tokenizer(src: string): WikilinkToken | undefined {
    const match = /^!?\[\[([^\]\n]+?)\]\]/.exec(src);
    if (!match) return undefined;
    const inner = match[1];
    const pipe = inner.indexOf("|");
    const targetPart = pipe === -1 ? inner : inner.slice(0, pipe);
    const aliasPart = pipe === -1 ? "" : inner.slice(pipe + 1);
    // Drop `#heading` from the link text when no explicit alias is given.
    const target = targetPart.split("#")[0].trim();
    const alias = (aliasPart || targetPart).trim();
    return { type: "wikilink", raw: match[0], target, alias };
  },
  renderer(token: Tokens.Generic) {
    const { target, alias } = token as WikilinkToken;
    const exists = resolveExists ? resolveExists(target) : false;
    const cls = exists ? "wikilink" : "wikilink wikilink-missing";
    return `<a class="${cls}" data-wikilink="${escapeHtml(target)}">${escapeHtml(alias)}</a>`;
  },
};

marked.use({ extensions: [wikilinkExtension] });

/**
 * Render markdown to sanitized HTML. Runs client-side only (the app is SPA /
 * `ssr = false`), so `DOMPurify` always has a real DOM to work with.
 *
 * `resolve` lets the caller mark which `[[wikilinks]]` point at a known brief;
 * resolved links get `.wikilink`, dangling ones also get `.wikilink-missing`.
 */
export function renderMarkdown(
  md: string,
  resolve?: (target: string) => boolean,
): string {
  resolveExists = resolve ?? null;
  try {
    const html = marked.parse(md ?? "", { async: false }) as string;
    // Keep the wikilink hook attribute through sanitization.
    return DOMPurify.sanitize(html, { ADD_ATTR: ["data-wikilink"] });
  } finally {
    resolveExists = null;
  }
}
