// Front-end-only helper for the brief-bootstrap chooser's "Paste a prompt"
// method (no backend). It builds a copy-paste handoff prompt that spells out the
// EXACT WAID file shape, so the model's output loads cleanly into edit mode. The
// template REQUESTS plain ## Current State / ## Open Questions sections (WAID's
// import step lifts them into the app-owned marker regions) and only forbids the
// Activity/Captures regions and raw markers. Because save_brief round-trips the
// whole raw file and parse_brief tolerates extra/missing keys, a slightly-off
// paste still loads — but the strict template minimizes drift.

import type { Brief } from "./types";

/** Whether a brief still has the freshly-created stub body (just an H1 +
 *  "Project context goes here."), i.e. it's a good candidate for bootstrap.
 *  Drives the "Generate initial brief" empty-state CTA in ProjectDetail. */
export function isStubBrief(brief: Brief): boolean {
  const body = brief.body
    .replace(/^#\s+.*$/m, "") // drop the H1 title
    .replace(/project context goes here\.?/i, "") // the create_brief placeholder
    .trim();
  return body.length === 0;
}

/** Build the paste-a-prompt template for a project name. */
export function pastePromptTemplate(name: string): string {
  return `You are helping me write the initial WAID project brief for "${name}".
Output a single Markdown file with YAML frontmatter, in EXACTLY this shape:

---
name: ${name}
status: active
description: <one factual sentence>
tags: [<0-6 short lowercase tags>]
links:
  - label: <name>
    url: <url>
---

# ${name}

<overview paragraph; then optional ## Goals / ## Stack / ## Notes sections>

## Current State

<2-4 sentences on where the project stands right now>

## Open Questions

- <a question a maintainer should resolve next>

Rules:
- Base everything ONLY on the project context you have access to. Don't invent.
- DO include the ## Current State and ## Open Questions sections above when you
  can — they're the most useful part of the brief. Omit a section only if you
  genuinely have nothing for it.
- Do NOT add ## Activity or ## Captures, and do NOT write any <!-- waid:... -->
  comment markers — WAID manages those.
- Output only the file contents, no commentary or code fences.`;
}
