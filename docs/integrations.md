# Project-management integrations

Connect a brief to a PM tool and the detail pane shows your live tasks /
notifications inline, with a one-line local rollup (counts by status, recently
updated). Supported providers: **Linear, Jira, Asana, GitHub, Notion, Gmail,
Slack, Figma** — plus
**[NeuroSkill](https://github.com/NeuroSkill-com/skill)**, a local EEG link that writes a
`## Mind State` body region instead of panel items (see
[mind-state.md](mind-state.md)).

Everything here is deterministic and **strictly additive**: fetches never write
into your `.md`, so a failed or auth-walled fetch is just a toast and a panel
error state — the brief renders fully regardless.

## How it's wired

A brief owns one or more **connections** (account-level metadata: provider,
label, and where needed a base URL or account) and one or more **integration
selectors** (a `kind` — `tasks`, `notifications`, GitHub's `pulls` / `commits`,
Notion's `page`, Gmail's `email`, Slack's `messages`, Figma's `comments` — plus
an optional `query` and `limit`). A connection's *metadata* lives in the
brief's frontmatter and round-trips with the file; its *token* lives in the OS
keyring, scoped per brief. One connection can carry several feeds — e.g. two
Notion databases — since a feed's identity is `(connection, kind, query)`.

Manage them from the **integrations** button on a brief: add/edit/delete
connections and feeds, paste a token, and *Test connection* to validate it. A
**help icon** beside each token field opens inline setup steps, the required
scopes, and a link to that provider's docs. In the manage view you can also
**rename a connection** and **edit a feed's query in place**.

Tokens are read-only API tokens you create in each provider; only GitHub
exposes notifications, Jira needs a base URL + account email, Asana needs a
workspace id, and GitHub Enterprise needs a base URL.

## Auto-sync

Off by default. Turn on **Auto-sync** in Settings → Sync & GitHub (not to be
confused with **Sync on open**, which refreshes a brief's `## Activity` block
when you open it) and a background poller re-runs each feed's normal fetch, so
new items show up without opening the brief:

- **Where "new" shows** — an accent dot + count on the brief in the sidebar
  and the widget roster, a total in the resting widget's header, and
  "· N new" on the feed line. Opening the brief (or its widget leaf) marks the
  items seen; the new rows stay highlighted until you leave it.
- **How often** — per brief: the one you're viewing every 20 s, other active
  or blocked briefs every 60 s, paused every 10 min, archived never. Slower
  kinds are capped by provider rate limits (email and Slack 30 s, GitHub
  notifications and Figma comments 60 s), each provider is spaced out, and
  errors or rate limits back off automatically. Hover the ⓘ next to the
  toggle for the current rates.
- **What it never does** — write to a `.md`, call the LLM, or toast. A failed
  background fetch just shows the panel's usual "Couldn't refresh" line.
- **What it stores** — only hashes of seen items, in `localStorage`
  (`waid-feed-seen`), so a restart flags what arrived while WAID was closed.
  No titles, URLs, queries or paths. Turning Auto-sync off deletes it.

Notion pages and NeuroSkill feeds are never polled. Your own Slack messages
that match a feed's query count as new (WAID doesn't know which Slack user is
you) — add `-from:me` to the query to exclude them.

## Secret storage

Tokens and keys live in the platform keychain, never in settings, env, or the
`.md` files. Keys:

- `github.token` — GitHub token for private-repo brief sync
- `anthropic.api_key` — Anthropic API key for synthesis
- `openai.api_key:<origin>` — optional API key for the OpenAI-compatible
  synthesis endpoint, scoped to the endpoint URL's origin (scheme + host +
  non-default port) so a saved key never follows a host change
- `bconn:<brief-path>:<id>` — per-brief PM connection tokens
- `whook:<brief-path>:<id>` — per-brief webhook secrets
- `gmail.client_id` / `gmail.client_secret` — the bring-your-own Google
  Desktop OAuth client
- `gmail.oauth:<account-email>` — the account-scoped Gmail OAuth grant, shared
  across briefs (not `bconn:`-keyed)

## GitHub

Four feed kinds. `tasks` and `notifications` hit the REST API (open issues/PRs
assigned to you; unread notifications). `pulls` and `commits` are
**search-backed** — `pulls` lists PRs matching a `query` (default `is:pr
is:open author:@me`) and `commits` lists commits matching a `query` (default
`author:@me`); they're independent feeds, so a brief can carry either, both, or
neither.

Because GitHub search returns *public* matches regardless of a token's repo
grant, **every GitHub feed must name its repos** — set one or more `owner/name`
repos on the connection. WAID injects them as `repo:` qualifiers on search
feeds and filters the REST feeds down to them; an unscoped feed is
**rejected**, never silently broadened to every repo you've ever touched. (A
power-user `query` that already pins scope with a `repo:` / `org:` / `user:`
qualifier is honored verbatim.)

GitHub **notifications** require a *classic* personal access token with the
`notifications` scope (fine-grained tokens can't reach the endpoint — WAID says
so on a 403); the Enterprise base-URL field is ignored if you point it at
public github.com.

## Notion

A connection (a Notion *internal integration* token) can pull a **database's
rows** as items (`kind: tasks`), or treat a **page** (`kind: page`) as context
that feeds AI synthesis — its text joins the evidence pool just like a
`notion.so` link in the body. The integration only sees databases/pages you've
explicitly *shared* with it via the page's *Connections* menu.

## Gmail

Gmail surfaces recent, brief-relevant emails (`kind: email`): the selector's
`query` is a **Gmail search string** (`from:acme.com subject:"redesign"
newer_than:14d`), and matching messages appear as items (subject as the title,
sender in the assignee slot, read/unread status). It's **read-only** and
**metadata-only** — only subjects and short snippets ever leave Gmail, never
message bodies. Unlike the token-paste providers, Gmail signs in with **Google
OAuth** (it opens your browser once per Gmail account; the grant is stored in
the OS keyring and reused across every brief pointed at that account).

Don't know Gmail's search operators? The feed form offers one-click **search
templates** (recent unread, needs my reply, important, starred, …) and a
**"describe it" box** that turns plain English ("unread from my manager this
week") into a query via the configured synthesis LLM — display-only, it just
fills the field for you to tweak.

### One-time Google Cloud setup (per the user, not WAID)

WAID ships no shared Google credentials — you bring your own OAuth client:

1. Create (or reuse) a Google Cloud project and **enable the Gmail API**.
2. **OAuth consent screen:** User type **External**; add the scope
   `.../auth/gmail.readonly`. **Set the publishing status to "In production."**
   Leave it *unverified* — for personal use (< 100 users) you just click through
   the "Google hasn't verified this app" warning at consent time; verification
   is **not** required.
   - ⚠️ **This is the line everyone gets wrong.** `gmail.readonly` is a
     *restricted* scope, so in **Testing** status Google **revokes the refresh
     token after 7 days** (`invalid_grant`) — you'd have to reconnect every
     week. **Production** (even unverified) gives a long-lived grant.
3. **Credentials → Create OAuth client ID → Application type: Desktop app.**
   Copy the **client ID** and **client secret**.
4. In WAID **Settings → Gmail (Google OAuth client)**, paste the ID + secret
   (stored in the keyring). Loopback redirect URIs (`http://127.0.0.1:<port>`)
   are auto-allowed for Desktop clients, so there's nothing to register.

Then, on a brief, pick **Gmail** in the integrations modal, click **Connect
Google account**, approve in the browser, and add an email feed with a search
query.

## Slack

Slack surfaces recent messages matching a search (`kind: messages`): the
selector's `query` is a **Slack search string** (`in:#waid from:@dana
after:2026-06-01`), and matching messages appear as items (the message text as
the title, sender in the assignee slot, the channel and a cleaned-up snippet
alongside). The read path is `search.messages` over whatever the *user* can
see — scoped by the query, not by bot membership. Message feeds flow into the
panel, digest, and morning briefing, but never into synthesis evidence (same
deliberate exclusion as Gmail). Unlike Gmail, Slack is a plain **token-paste**
provider: you paste a user token and *Test connection*, no OAuth dance. As with
Gmail, the feed form offers one-click **search templates** and a **"describe
it" box** that turns plain English into a query via the configured synthesis
LLM.

### One-time Slack setup (per the user, not WAID)

WAID ships no shared Slack app:

1. Create a Slack app at [api.slack.com/apps](https://api.slack.com/apps) →
   *From scratch*, in your workspace.
2. **OAuth & Permissions → User Token Scopes** → add `search:read`. (User Token
   Scopes, **not** Bot Token Scopes — bot tokens can't search.)
3. **Install to Workspace**, approve, and copy the **User OAuth Token**
   (`xoxp-…`) from the same page.
4. Paste it as the token when adding a Slack connection in WAID.

⚠️ Do **not** enable the app's *Token Rotation* setting — it converts the token
to an expiring `xoxe.xoxp-…` that dies in 12 hours, and WAID's token-paste
connections don't refresh. Without rotation the token lives until you revoke it
or uninstall the app. One app per workspace; a second workspace means repeating
these steps there.

## Figma

Figma surfaces a file's comments (`kind: comments`): the selector's `query` is
**required** and carries a **Figma file URL or key**
(`figma.com/design/AbC123/…` or the bare `AbC123`) — the scope, because
Figma's REST API has **no cross-file comment search**, so a feed is scoped to
one file (the same call as GitHub's required repo scoping). Each comment maps
to an item (the first line as the title, the author handle in the assignee
slot, an `Open`/`Resolved` status, and a snippet alongside); the item link
lands you in the file, since the API exposes no per-comment anchor.

By default a feed shows recent **unresolved** comments; add **`mentions:me`**
(or `@me`) anywhere in the query to keep only comments that @-mention you. That
filter is **best-effort** — Figma has no structured mention field, so WAID
matches your handle against the comment text, which can miss renamed/group
mentions and occasionally catch your name typed in prose. Comment feeds flow
into the panel, digest, and morning briefing, but never into synthesis evidence
(same exclusion as Gmail/Slack). Like Slack, Figma is a plain **token-paste**
provider — no OAuth dance.

### One-time Figma setup (per the user, not WAID)

WAID ships no shared Figma app:

1. In Figma, open **Settings → Account → Personal access tokens** (or
   [figma.com/developers/apps](https://www.figma.com/developers/apps)).
2. **Generate new token**, name it (e.g. "WAID"), and grant the
   **`file_comments:read`** scope (and `current_user:read` for the @-me
   filter).
3. Copy the token (it's a *user* token, sent in the `X-Figma-Token` header,
   read-only) and paste it when adding a Figma connection in WAID.
