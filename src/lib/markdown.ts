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

/**
 * Wrap each `##` heading and the content beneath it (up to the next `##`) in a
 * native `<details class="md-section">` so sections collapse. The `#` title
 * is treated the same way, so the intro paragraph under it folds too; `###`+
 * headings stay inside their parent section. Anything before the first `#`/`##`
 * is left untouched. Returns the input unchanged when the body has neither.
 *
 * Takes the *sanitized* output of `renderMarkdown` and only re-parents those
 * nodes, adding elements/attributes it controls (`details`/`summary`/`div`,
 * `class`, `data-section`), so the result stays safe for `{@html}`.
 *
 * `data-section` is a key derived from the heading text (lower-cased; a `#n`
 * suffix disambiguates repeated headings) that callers use to persist which
 * sections are collapsed. `isCollapsed(key)` decides the initial open state.
 * Keys are checked against every key already emitted, so a literal heading
 * such as `Foo#2` cannot collide with the suffix given to a repeated `Foo`.
 */
export function sectionize(html: string, isCollapsed: (key: string) => boolean): string {
  const tpl = document.createElement("template");
  tpl.innerHTML = html;
  if (!tpl.content.querySelector("h1, h2")) return html;

  const out = document.createElement("div");
  const used = new Set<string>();
  let body: HTMLElement | null = null;

  for (const node of Array.from(tpl.content.childNodes)) {
    const tag = node.nodeType === Node.ELEMENT_NODE ? (node as Element).tagName : "";
    if (tag !== "H1" && tag !== "H2") {
      (body ?? out).appendChild(node);
      continue;
    }
    const base = (node.textContent ?? "").trim().toLowerCase();
    let key = base;
    for (let n = 2; used.has(key); n++) key = `${base}#${n}`;
    used.add(key);

    const details = document.createElement("details");
    details.className = "md-section";
    details.dataset.section = key;
    details.open = !isCollapsed(key);
    const summary = document.createElement("summary");
    summary.appendChild(node);
    body = document.createElement("div");
    body.className = "md-section-body";
    details.append(summary, body);
    out.appendChild(details);
  }
  return out.innerHTML;
}
