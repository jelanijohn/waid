# Mind State (NeuroSkill, optional)

Connect a brief to a local
**[NeuroSkill](https://github.com/NeuroSkill-com/skill)** EEG dashboard and
WAID maintains a deterministic `## Mind State` region in the brief — a
rolling-window (`today` / `7d` / `14d` / `30d`, default **14d**) aggregate of
your focus / engagement / mood / relaxation over that project's **labeled**
work sessions, with a per-metric trend and your deepest-focus block:

```markdown
## Mind State
_Rolling 14 days · 4 sessions · 3.2 hrs tracked · updated 2026-06-19_

| Metric      | Mean | Trend |
|-------------|-----:|:-----:|
| Focus       |   41 |   ↑   |
| Engagement  |   37 |   ↑   |
| Mood        |   62 |   →   |

Deepest focus: Tue 2pm–3pm (focus 58).
```

## Attribution is authored, not guessed

The hard problem is **attribution** — an EEG reading at 2:47pm is about you,
not a project. WAID solves it by *authoring* the answer: when you launch into
work on a brief (open a link/webhook, or hit **Start session**) it writes a
`waid:brief=<slug>:start` label into NeuroSkill, and a matching `:end` when you
end the session, navigate away, or close the app. `## Mind State` then
aggregates only the EEG epochs inside those labeled windows. (There's no
guessing from window / terminal / browser activity — that's deliberately out of
scope.)

NeuroSkill's EEG table has **no project column** — an epoch belongs to a brief
purely because its timestamp falls inside one of that brief's labeled windows,
matched on the **exact** `waid:brief=<slug>` prefix, so a sibling project's
sessions never bleed in. One practical corollary: if you forget to end a
session, up to **4 hours** of EEG attributes to that brief before the interval
cap closes it — the **End session** button and the on-close `:end` exist to
keep the windows honest.

## How it differs from the other connectors

- **Local, token-authenticated** — NeuroSkill runs on your machine; WAID
  reaches its daemon over localhost. The daemon gates its API with a bearer
  token it writes to disk (`…/skill/daemon/auth.token`), which WAID reads at
  call time — nothing for you to paste. Just pick it in the integrations modal;
  optionally override the data directory, daemon URL, or token path.
- **Read-only and tightly scoped** — WAID reads NeuroSkill's local SQLite
  read-only and touches **only** the EEG timeseries and your own WAID session
  labels. It never reads window titles, terminal/browser history, clipboard, or
  files, so toggling NeuroSkill's capture settings on/off makes no difference.
  The only thing it *writes* is the session label, via the daemon's local API.
- **A body region, not panel items** — the feed shows as a slim status row with
  a refresh that regenerates `## Mind State` (like `## Activity`), not a list
  of cards. It's deterministic (no LLM) and replaced in place, so frontmatter
  and the rest of your brief are preserved byte-for-byte.
- **Feeds AI synthesis (optional)** — if you have an LLM provider configured,
  the Mind State rollup also joins the evidence behind `## Current State` as a
  short *descriptive* readout (worded trends, no raw data), so a synthesized
  summary can note when you've been deep in focus on this project. Only for
  briefs with a NeuroSkill feed; numbers only, framed as context rather than a
  directive.

## WSL2 note

NeuroSkill runs on the Windows host; point the data directory at the
`/mnt/c/...AppData/Local/NeuroSkill` path and the token path at the
Windows-host `…/AppData/Roaming/skill/daemon/auth.token`. WAID reaches the
daemon on `127.0.0.1:18444` (override the daemon URL if it differs); from WSL2
that requires mirrored networking with host loopback so localhost reaches the
Windows daemon (`.wslconfig`: `networkingMode=mirrored` + `[experimental]
hostAddressLoopback=true`). Detailed findings from the WSL2 bring-up live in
[neuroskill_wsl_findings.md](neuroskill_wsl_findings.md).

## Licensing boundary

NeuroSkill and WAID are both GPL-3.0. WAID nonetheless stays strictly at the
process/file boundary (reads its data files, calls its local HTTP API) and
links no NeuroSkill code — an architectural choice, not a license requirement.
