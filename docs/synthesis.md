# AI synthesis, digests & the morning briefing (optional)

All AI features are optional and off until you configure a provider. Synthesis
turns the deterministic fetchers into *evidence* a model reasons over, then
writes a short summary back into two app-owned regions of the brief (see
[brief-format.md](brief-format.md#app-owned-regions) for the full region
table): `## Current State` and the inner block of `## Open Questions`. The
markers are HTML comments, so they render to nothing and the files stay
portable / Obsidian-native. Everything outside the markers — your prose, your
`## Captures`, all frontmatter — is preserved byte-for-byte.

Evidence comes from the brief's links, Notion page text, your `## Captures`
notes, and (if connected) the
[NeuroSkill](https://github.com/NeuroSkill-com/skill) Mind State rollup.

## Configuring a provider

In the settings popover (the ⚙/tune button) under *AI synthesis*:

- **Ollama (local)** — runs against a local [Ollama](https://ollama.com)
  server. Set the base URL (default `http://localhost:11434`) and pick a pulled
  model. Nothing leaves your machine.
- **Anthropic (cloud)** — set a model and save an API key (stored in the OS
  keyring). Sends evidence to the Anthropic API.
- **OpenAI-compatible (custom)** — any server speaking OpenAI's
  `chat/completions` API: OpenRouter, Groq, Mistral, LM Studio, llama.cpp
  server, vLLM, Ollama's own `/v1` facade, … Set the base URL **including any
  `/v1` your server uses** (WAID appends `/chat/completions`; pasting the full
  endpoint also works), the model id as your provider documents it, and
  optionally an API key (OS keyring; local servers need none). A `localhost`
  URL keeps everything on your machine; a remote URL sends brief text and
  fetched evidence to that server, under that operator's data policy.

Then hit **Refresh** on a brief (or *Sync all* in the titlebar). Synthesis is
manual — there's no auto-sync on open or timer.

## Digest & morning briefing

With a provider configured, you can also generate a short prose **digest** of
one brief's live PM items (snapshot-able into `## Captures`), or a cross-brief
**morning briefing** over every project's items. Both are display-only — they
re-fetch the live items and never write the `.md` unless you snapshot.

## Open Questions — known tradeoff

The model regenerates only the *inner* `waid:questions` block, wholesale, each
run (so it can retract resolved questions). Your own questions live *above* the
block and survive untouched, but anything you type *inside* the block is
overwritten on the next synthesis — answer questions or add your own above it.

## Safety

All fetched content (web pages, source JSON, GitHub data, Notion page text,
your Captures) is treated as **data, never instructions**. The model's output
schema is closed to two fields, so it structurally cannot change status, links,
tags, or webhooks, or fire anything. Web fetches are GET-only and truncated.
Any error (fetch, provider, or unparseable output) leaves the `.md` untouched.
