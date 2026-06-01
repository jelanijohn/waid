---
name: Sample Web App
status: active
description: Example brief for a web application — shows links and webhooks
tags: [sample, web, example]
links:
  - label: Claude Project
    url: https://claude.ai/project/example
  - label: GitHub
    url: https://github.com/your-org/sample-web-app
  - label: Docs
    url: https://example.com/docs
webhooks:
  - label: Deploy staging
    url: https://example.com/hooks/deploy-staging
    method: POST
    body: '{"env": "staging"}'
last_opened: 2026-05-31T10:00:00Z
---

# Sample Web App

This is a placeholder brief that ships with a fresh install so you can see the
format immediately. Replace it (or delete it) with your own project.

It demonstrates an **active** project that has both quick **links** (open a
Claude project, a repo, docs) and a **webhook** button (fire a deploy from the
detail pane).

## Current state

- Core features in progress.
- Wiring up CI/CD and a staging environment.

## Decisions log

- **Frontmatter is optional.** Every field can be omitted; half-written briefs
  still load.
- **Plain markdown.** Briefs stay portable and open in any editor.

## Open questions

- What belongs in a brief vs. a fuller spec doc?

## Captures
