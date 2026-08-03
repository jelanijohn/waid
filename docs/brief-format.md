# Brief format

Each project is a single markdown file: YAML frontmatter + a markdown body.
Every field is optional — half-written files still load. The format is
deliberately standard (Obsidian-native): no proprietary keys are required, and
WAID's write paths round-trip the whole file, so unknown frontmatter keys and
ordering survive every edit.

```markdown
---
name: Sample Web App
status: active            # active | paused | blocked | archived (free-form; rendered as a pill)
description: Example project brief
tags: [sample, web, example]
links:
  - label: Claude Project
    url: https://claude.ai/project/xxx
  - label: GitHub
    url: https://github.com/...
webhooks:
  - id: deploy-staging    # stable slug; scopes the keyring secret, survives relabel
    label: Deploy staging
    url: https://...
    method: POST          # defaults to POST if omitted
    body: '{"env":"staging"}'   # optional; sent as JSON
    headers:              # optional; one value may interpolate {{secret}} from the keyring
      - name: Authorization
        value: "Bearer {{secret}}"
connections:              # PM connections owned by this brief (metadata only; tokens live in the keyring)
  - id: linear-personal
    provider: linear      # linear | jira | asana | github | notion | gmail | slack | figma | neuroskill
    label: Linear (personal)
integrations:             # selectors referencing the connections above
  - connection: linear-personal
    kind: tasks           # tasks | notifications | pulls | commits (GitHub) | page (Notion) | email (Gmail) | messages (Slack) | comments (Figma) | mind (NeuroSkill)
    query: "assignee:me"
    limit: 10
last_opened: 2026-05-31T10:00:00Z
---

# Project context

The full markdown brief lives here.
```

Connection *metadata* lives on the brief (portable, Obsidian-safe); the *token*
lives in the OS keyring, scoped per brief. See
[integrations.md](integrations.md).

## Webhooks

Webhook buttons in the brief's launch row fire GET/POST (and PUT/PATCH/DELETE)
requests with a toast on success/failure. Webhooks carry **custom headers**;
one header value may reference a keyring-backed secret via a `{{secret}}`
sentinel (e.g. `Authorization: Bearer {{secret}}`), resolved backend-side at
fire time. The header *shapes* live in the brief's frontmatter (edited in Edit
mode); the secret lives in the OS keyring (`whook:<brief-path>:<id>`). Firing
only makes an HTTP request — it never writes the `.md`.

## App-owned regions

Sync and synthesis write only inside HTML-comment markers, so the files stay
portable and everything outside them — your prose, your `## Captures`, all
frontmatter — is preserved byte-for-byte:

| Region | Markers | Written by |
|---|---|---|
| `## Activity` | `waid:sync:start/end` | Brief sync (deterministic — no LLM) |
| `## Mind State` | `waid:mind:start/end` | NeuroSkill rollup (deterministic — no LLM) |
| `## Current State` | `waid:state:start/end` | Synthesis (regenerated wholesale) |
| `## Open Questions` → inner block | `waid:questions:start/end` (nested) | Synthesis (inner block only) |
