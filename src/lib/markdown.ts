import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * Render markdown to sanitized HTML. Runs client-side only (the app is SPA /
 * `ssr = false`), so `DOMPurify` always has a real DOM to work with.
 */
export function renderMarkdown(md: string): string {
  const html = marked.parse(md ?? "", { async: false }) as string;
  return DOMPurify.sanitize(html);
}
