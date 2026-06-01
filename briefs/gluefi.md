---
name: Gluefi
status: active
description: Fintech / Web2.5 infrastructure for tokenized finance, Japan focus
tags: [fintech, web3, japan, infrastructure]
links:
  - label: Claude Project
    url: https://claude.ai/project/xxxx-gluefi
  - label: GitHub
    url: https://github.com/your-org/gluefi
  - label: Notion
    url: https://notion.so/gluefi
webhooks:
  - label: Deploy staging
    url: https://example.com/hooks/gluefi/deploy-staging
    method: POST
    body: '{"env": "staging"}'
last_opened: 2026-05-31T10:00:00Z
---

# Gluefi

Fintech infrastructure for tokenized finance — the "glue" between traditional
finance rails and on-chain settlement. Web2.5: web2 UX, web3 settlement.
Primary market focus is **Japan**.

## Current state

- Defining the core ledger + custody abstractions.
- Evaluating regulatory posture for the Japanese market (JFSA).
- Early partner conversations.

## Decisions log

- **Web2.5 over pure web3.** Most target users don't want a wallet; abstract it.
- **Japan first.** Regulatory clarity + appetite for tokenized assets.

## Open questions

- Custody: self-custody vs. licensed partner?
- Which asset class to tokenize first (real estate, funds, receivables)?

## Captures
