---
name: update-docs
description: Update WAID's two project docs — README.md (user-facing) and waid-project-summary.md (portable Claude-project context summary) — to reflect code changes. Use after shipping a feature, adding a backend command / PM provider / frontend component, or changing the data model, so the docs stay in sync with the code. Invoke when asked to "update the README and project summary", "sync the docs", or after a change that the docs should mention.
---

# Update WAID docs

Two root docs must stay in sync with the code. Update **both** unless told otherwise.

| File | Audience | Tone |
|---|---|---|
| `README.md` | Anyone landing on the repo (humans, GitHub) | Practical: setup, run/build, feature list, how-tos |
| `waid-project-summary.md` | A Claude project's knowledge base (dropped in as context) | Exhaustive snapshot: concept, architecture, data model, decisions |

These are different documents, not duplicates — the same change is described at different depth and framing in each. Don't copy text between them.

## Process

1. **Find what changed.** Read the diff since the docs were last touched, don't guess:
   ```bash
   git log --oneline -15
   git diff HEAD --stat          # uncommitted
   git log -p -- README.md waid-project-summary.md | head -5   # when docs last moved
   ```
   Focus on changes to: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (command registration), `src-tauri/src/provider/` (PM providers), `src/lib/tauri.ts` (command wrappers), `src/lib/types.ts` (data model), `src/lib/components/`.

2. **Read both docs fully** before editing — they have established structure and voice; match it.

3. **Map the change to the right sections** (see checklist below) and edit surgically with the Edit tool. Keep the existing line-wrap width (~80 cols), em-dash voice, and table/list style.

4. **Verify cross-references stay true:** keyring key formats, command names, the `generate_handler!` list, `types.ts` ↔ Rust struct mirror, provider list (Linear/Jira/Asana/GitHub/Notion/Gmail), file-tree blocks in both docs.

5. **Move shipped items.** When something graduates from planned → done, update the "Out of scope / planned" sections in *both* docs (README §"Out of scope for v1", summary §8) and the "shipped" callouts.

## Section map — where a change lands

**New backend command** (`#[tauri::command]`):
- summary §2 "Backend commands" table (add a row), and the `generate_handler!` note if registration pattern changed
- summary §3 if it adds/changes a type
- README feature bullet + relevant how-to section if user-visible
- `src/lib/tauri.ts` wrapper is the canonical command name — match it

**New / changed PM provider:**
- summary §5 (Design, per-provider subsection, the `provider/` file tree), §4 structure tree
- README "Project-management integrations" section + provider list everywhere ("Linear, Jira, Asana, GitHub, Notion, Gmail" appears in many spots — grep and update all)
- `providers.ts` `PROVIDERS` registry drives display metadata; mention if kinds/form-fields changed

**Data model change** (`types.ts` / Rust structs):
- summary §3 (the TS interface blocks) — keep mirroring `#[serde(rename_all = "camelCase")]`
- both docs' brief-format YAML examples if frontmatter keys changed

**New frontend component / UI:**
- summary §5 "UI", §6 "Visual Identity" if themed, the component lists in both file-tree blocks
- README feature bullet

**New keyring secret / key format:**
- both docs' keyring-key lists (README "Secret storage" bullet; summary §7 secret-storage bullet) — keep the exact key templates (`bconn:<brief-path>:<id>`, `whook:<brief-path>:<id>`, `gmail.oauth:<account-email>`, etc.)

**New command/script, prerequisite, or run/build step:**
- README "Run" / "Build" / "Prerequisites"; summary §4 "Run / build"
- keep `CLAUDE.md`'s Commands section in mind (it's separate but related)

## Don't

- Don't touch `CLAUDE.md` here — it's maintained separately (though check it for consistency).
- Don't invent features or restate things that didn't change.
- Don't reformat untouched sections or rewrap prose you didn't need to edit — keep the diff tight.
- Don't let the two docs drift into contradiction (e.g. provider list, planned-vs-shipped).
