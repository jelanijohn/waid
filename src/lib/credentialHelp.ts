// Setup instructions for every credential WAID asks the user to paste or grant.
// Rendered by CredentialHelp.svelte: a help icon beside each token field opens a
// popup with these steps + required scopes + a link to the provider's docs.
//
// Keys match the provider ids used in IntegrationsModal (linear/jira/asana/
// github/notion/slack/gmail) plus the standalone Settings credentials
// (gmail-client, anthropic) and the webhook secret sentinel. Scopes are kept in
// sync with what the Rust backend actually requires (see src-tauri/src/provider/
// and commands.rs).

export type CredentialHelp = {
  title: string;
  steps: string[];
  scopes?: string[];
  note?: string;
  docsUrl?: string;
};

export const credentialHelp: Record<string, CredentialHelp> = {
  linear: {
    title: "Linear API token",
    steps: [
      "Open Linear and go to Settings → Security & access.",
      "Under “Personal API keys”, click “New API key”.",
      "Name it (e.g. “WAID”) and create it.",
      "Copy the key and paste it here.",
    ],
    scopes: ["read"],
    note: "A read-only key is enough — WAID only fetches issues, never writes.",
    docsUrl: "https://linear.app/docs/api-and-webhooks",
  },
  jira: {
    title: "Jira API token",
    steps: [
      "Go to id.atlassian.com → Security → “Create and manage API tokens”.",
      "Click “Create API token”, name it, and copy the value.",
      "Paste the token here, and enter the Atlassian account email it belongs to.",
      "Set the Base URL to your site, e.g. https://acme.atlassian.net.",
    ],
    note: "Jira authenticates with your email + token together (Basic auth), so all three fields are required.",
    docsUrl:
      "https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/",
  },
  asana: {
    title: "Asana personal access token",
    steps: [
      "Open Asana → your profile photo → Settings → Apps.",
      "Click “Manage developer apps”, then “Create new token” (Personal access tokens).",
      "Name it, agree to the terms, and copy the token.",
      "Paste it here. The Workspace ID is optional — find it in the URL when viewing your workspace.",
    ],
    docsUrl: "https://developers.asana.com/docs/personal-access-token",
  },
  github: {
    title: "GitHub personal access token",
    steps: [
      "Go to github.com → Settings → Developer settings → Personal access tokens.",
      "Create a token (Fine-grained or “Tokens (classic)”).",
      "Grant read access to the repositories you want WAID to see.",
      "Copy the token and paste it here.",
    ],
    scopes: ["repo (classic)", "Contents: read-only (fine-grained)"],
    note: "Only needed for private repositories — public repos work without a token.",
    docsUrl:
      "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens",
  },
  notion: {
    title: "Notion integration token",
    steps: [
      "Go to notion.so/my-integrations and click “New integration”.",
      "Choose “Internal” integration and create it.",
      "Copy the “Internal Integration Secret” and paste it here.",
      "Important: open each Notion page/database you want WAID to read, and share it with this integration (••• → Connections).",
    ],
    note: "An integration can only see pages explicitly shared with it.",
    docsUrl: "https://developers.notion.com/docs/create-a-notion-integration",
  },
  slack: {
    title: "Slack user token",
    steps: [
      "Go to api.slack.com/apps and create (or open) an app in your workspace.",
      "Under “OAuth & Permissions”, add the User Token Scope below.",
      "Install (or reinstall) the app to your workspace.",
      "Copy the “User OAuth Token” (starts with xoxp-) and paste it here.",
    ],
    scopes: ["search:read"],
    note: "WAID needs a user token (xoxp-…), not a bot token — search runs as you.",
    docsUrl: "https://api.slack.com/authentication/token-types#user",
  },
  figma: {
    title: "Figma personal access token",
    steps: [
      "Open Figma → Settings → Account → “Personal access tokens” (or figma.com/developers/apps).",
      "Click “Generate new token” and name it (e.g. “WAID”).",
      "Grant the “file_comments:read” scope (and “current_user:read” for the @-me filter).",
      "Copy the token and paste it here.",
    ],
    scopes: ["file_comments:read", "current_user:read"],
    note: "A read-only user token (sent as X-Figma-Token). A feed is scoped to one file URL/key — Figma has no cross-file comment search. Add `mentions:me` to the query to keep only comments that tag you (best-effort name match: it can miss renamed/group mentions and catch your name typed in prose).",
    docsUrl: "https://developers.figma.com/docs/rest-api/comments-endpoints/",
  },
  gmail: {
    title: "Connect Gmail",
    steps: [
      "First set up a Google OAuth client in Settings → “Gmail (Google OAuth client)” — see the help icon there (one-time).",
      "Click “Connect Google account”; your browser opens Google sign-in.",
      "Approve the read-only access request.",
      "WAID stores the grant in your OS keychain and the window returns automatically.",
    ],
    scopes: ["gmail.readonly"],
    note: "Read-only and metadata-only — WAID reads subjects/snippets to build a brief, never message bodies.",
    docsUrl: "https://developers.google.com/gmail/api/auth/scopes",
  },
  "gmail-client": {
    title: "Google OAuth client (Desktop)",
    steps: [
      "Open Google Cloud Console and create (or pick) a project.",
      "Enable the Gmail API for the project (APIs & Services → Library).",
      "Configure the OAuth consent screen; set publishing status to “In production”.",
      "Go to APIs & Services → Credentials → Create credentials → OAuth client ID.",
      "Choose application type “Desktop app”, create it, then copy the Client ID and Client secret here.",
    ],
    note: "Must be a “Desktop app” client. In “Testing”, Google revokes the grant after 7 days, so use “In production” (unverified is fine for personal use).",
    docsUrl: "https://developers.google.com/identity/protocols/oauth2/native-app",
  },
  anthropic: {
    title: "Anthropic API key",
    steps: [
      "Go to console.anthropic.com and sign in.",
      "Open Settings → API keys.",
      "Click “Create Key”, name it, and copy the value (starts with sk-ant-).",
      "Paste it here.",
    ],
    note: "Used only for the optional AI synthesis of Current State & Open Questions.",
    docsUrl: "https://console.anthropic.com/settings/keys",
  },
  neuroskill: {
    title: "NeuroSkill (local)",
    steps: [
      "Make sure the NeuroSkill desktop app / daemon is running on this machine.",
      "Optional: set the data directory if your NeuroSkill data isn't at the default AppData path.",
      "Optional: set the daemon URL if it listens somewhere other than http://127.0.0.1:18444.",
      "Optional: set the auth token path if the daemon's auth.token file isn't at the default OS location.",
    ],
    note: "WAID reads NeuroSkill's local SQLite read-only (only the EEG timeseries + your WAID session labels) to build a deterministic ## Mind State region, and writes a session label to the daemon's local API (authenticated with the daemon's own bearer token) when you launch/end work. On WSL2 the daemon runs on the Windows host — point the daemon URL, data directory, and token path at the Windows-host paths (the token file lives under AppData/Roaming/skill/daemon).",
    docsUrl: "https://github.com/neuroskill/neuroskill",
  },
  "webhook-secret": {
    title: "Webhook secret",
    steps: [
      "Put the literal {{secret}} placeholder anywhere in a header value, e.g. Authorization: Bearer {{secret}}.",
      "Enter the actual secret value in this field.",
      "When the webhook fires, WAID substitutes {{secret}} with the stored value.",
    ],
    note: "The secret is stored in your OS keychain and is never written into the brief file.",
  },
};
