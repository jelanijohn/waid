# Using an Obsidian vault

Point the briefs folder at your vault (or any subfolder of it) via **Change**
at the bottom of the sidebar. WAID then:

- **Scans recursively** — every `.md` under the folder becomes a brief, while
  Obsidian's `.obsidian/` (and other dot-folders like `.trash/`, `.git/`) are
  skipped.
- **Renders `[[wikilinks]]`** in the brief body — links to other briefs are
  clickable (they select that brief); dangling links are shown muted. Aliases
  (`[[note|label]]`) and heading anchors (`[[note#section]]`) are supported.
- **Shows backlinks** — a "Linked from" list of every other brief that
  wikilinks to the current one.
- **Opens in Obsidian** — when the folder is inside a vault, an *Open in
  Obsidian* button deep-links the brief via `obsidian://open` so you can edit it
  with Obsidian's full editor.

## "Open in Obsidian" — one-time setup

The button uses an `obsidian://open?vault=<name>&file=<path>` deep link, which
only works if **Obsidian already knows the vault**. Two requirements:

1. **The vault has been opened in Obsidian at least once** (so it's registered).
   If you point WAID at a vault you actively use in Obsidian, this is already
   true and the button just works. If you've never opened the folder in
   Obsidian, do it once — *Open folder as vault* — or you'll get a "Vault not
   found" popup.
2. **The vault's name matches its folder name.** WAID sends the vault root's
   folder name (the folder containing `.obsidian/`). If you renamed the vault
   inside Obsidian to something else, the link won't resolve.

WAID can't auto-register the vault for you — there's no reliable cross-platform
way to do that from outside Obsidian, so this stays a one-time manual step.

> **WSL note:** running WAID as a Linux build under WSL while using Obsidian on
> Windows works, but the vault must live on the **Windows** filesystem (e.g.
> `C:\Users\you\Vault`, which WAID reads via `/mnt/c/...`). Windows Obsidian
> can't watch a vault stored on the WSL side (`\\wsl.localhost\...`) — it fails
> to load with an `EISDIR` error. You'll also need a URL handler such as
> `wslview` (from the `wslu` package) so WAID can hand `obsidian://` links to
> Windows.
