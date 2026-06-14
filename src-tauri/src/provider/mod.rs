//! Project-management provider integrations (Linear, Jira, Asana, GitHub,
//! Notion, Gmail, Slack).
//!
//! An account-level **connection** (metadata in `settings.json`, token in the OS
//! keyring) plus a per-brief **selector** drive a fetch against a provider's API,
//! normalised into `IntegrationItem`s the detail pane renders. The token is
//! loaded by the command layer (`commands.rs`) and passed in here — this module
//! never touches the keyring or disk, so it stays pure and testable.
//!
//! Strictly additive (see the implementation plan's guardrails): a brief renders
//! fully when a fetch fails; the failure surfaces as a toast, never a panic and
//! never a write into the `.md`. Dependency-light: each provider is a plain JSON
//! request over the shared `reqwest` (rustls) — no provider SDKs, no GraphQL
//! client. One submodule per provider, dispatched on `Connection.provider`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod asana;
pub mod github;
pub mod gmail;
pub mod jira;
pub mod linear;
pub mod notion;
pub mod slack;

/// The supported providers. Serialized lowercase (`"linear"`) to mirror the
/// `Provider` string union in `types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Linear,
    Jira,
    Asana,
    Github,
    Notion,
    Gmail,
    Slack,
}

// --- Shared HTTP helpers (used by every provider submodule) ---------------

/// A reqwest client with a short connect timeout (dead host fails fast) and a
/// generous overall timeout. Built per request — cheap, and keeps each provider
/// free of shared state.
pub(crate) fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))
}

/// Read a JSON response, mapping auth failures and non-2xx into clear, provider-
/// labelled errors. GraphQL-style in-band errors (Linear) are checked by the
/// caller after this returns.
pub(crate) async fn read_json(
    resp: reqwest::Response,
    provider: &str,
) -> Result<serde_json::Value, String> {
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!(
            "{provider} rejected the credentials — check the connection in Settings."
        ));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{provider} returned {status}: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("invalid JSON from {provider}: {e}"))
}

/// An account-level connection to a provider. **Metadata only** — the API token
/// lives in the OS keyring keyed by `id`, never in this struct, `settings.json`,
/// or any brief. Briefs reference a connection by `id` (see `BriefIntegration`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// Stable slug, e.g. `"linear-personal"`. Also the keyring entry key.
    pub id: String,
    pub provider: Provider,
    /// Display name in the UI.
    pub label: String,
    /// Jira cloud instance / GitHub Enterprise base URL (provider-dependent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// e.g. a Jira account email — never the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// A brief's reference to a connection plus a provider-specific selector. Parsed
/// from frontmatter alongside `links` / `webhooks`, and round-tripped via the
/// brief's `raw` on save — no special write handling needed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BriefIntegration {
    /// References `Connection.id`.
    pub connection: String,
    /// `"tasks"` | `"notifications"`. Defaults to tasks when omitted.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Provider-specific selector (e.g. a Linear query). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Optional cap on the number of items fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

fn default_kind() -> String {
    "tasks".to_string()
}

/// The normalized item the frontend ever sees — a task or a notification reduced
/// to a common shape, regardless of provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationItem {
    pub id: String,
    pub title: String,
    /// Opened via the existing opener plugin.
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// ISO timestamp; rendered with the frontend's relative `time.ts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// `"task"` | `"notification"`.
    pub kind: String,
    /// Provider extras (priority, project, …). Sorted keys for determinism.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

/// A local (no-LLM) rollup of a fetch, computed in `summarize`.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSummary {
    pub total: u32,
    pub by_status: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<u32>,
    /// Items updated within the last 7 days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_recently: Option<u32>,
}

/// What `fetch_integration` returns: the normalized items, when they were
/// fetched, and the local rollup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationFetch {
    pub items: Vec<IntegrationItem>,
    pub fetched_at: String,
    pub summary: IntegrationSummary,
}

/// Dispatch a fetch to the right provider and attach the local rollup. The
/// caller supplies `fetched_at` (not read from the clock here) so this stays
/// deterministic. Unsupported providers return a clear error rather than panic.
pub async fn fetch(
    conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
    fetched_at: String,
) -> Result<IntegrationFetch, String> {
    let items = match conn.provider {
        Provider::Linear => linear::fetch(conn, sel, token).await?,
        Provider::Jira => jira::fetch(conn, sel, token).await?,
        Provider::Asana => asana::fetch(conn, sel, token).await?,
        Provider::Github => github::fetch(conn, sel, token).await?,
        Provider::Notion => notion::fetch(conn, sel, token).await?,
        Provider::Gmail => gmail::fetch(conn, sel, token).await?,
        Provider::Slack => slack::fetch(conn, sel, token).await?,
    };
    let summary = summarize(&items);
    Ok(IntegrationFetch {
        items,
        fetched_at,
        summary,
    })
}

/// Cheap credential check used by `test_connection`: hit a trivial authenticated
/// endpoint and confirm the token is accepted.
pub async fn validate(conn: &Connection, token: &str) -> Result<(), String> {
    match conn.provider {
        Provider::Linear => linear::validate(conn, token).await,
        Provider::Jira => jira::validate(conn, token).await,
        Provider::Asana => asana::validate(conn, token).await,
        Provider::Github => github::validate(conn, token).await,
        Provider::Notion => notion::validate(conn, token).await,
        Provider::Gmail => gmail::validate(conn, token).await,
        Provider::Slack => slack::validate(conn, token).await,
    }
}

/// Build the local rollup from normalized items, using the current clock for the
/// "updated recently" window. Pure inner `summarize_at` keeps the math testable.
pub fn summarize(items: &[IntegrationItem]) -> IntegrationSummary {
    summarize_at(items, chrono::Utc::now())
}

fn summarize_at(
    items: &[IntegrationItem],
    now: chrono::DateTime<chrono::Utc>,
) -> IntegrationSummary {
    let mut by_status: BTreeMap<String, u32> = BTreeMap::new();
    let mut updated_recently = 0u32;
    for it in items {
        if let Some(s) = &it.status {
            *by_status.entry(s.clone()).or_insert(0) += 1;
        }
        if let Some(u) = &it.updated_at {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(u) {
                if (now - t.with_timezone(&chrono::Utc)).num_days() < 7 {
                    updated_recently += 1;
                }
            }
        }
    }
    IntegrationSummary {
        total: items.len() as u32,
        by_status,
        overdue: None, // no due-date signal in v1
        updated_recently: Some(updated_recently),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(status: &str, updated_at: Option<&str>) -> IntegrationItem {
        IntegrationItem {
            id: "x".into(),
            title: "t".into(),
            url: "https://x".into(),
            status: Some(status.into()),
            assignee: None,
            updated_at: updated_at.map(String::from),
            kind: "task".into(),
            meta: BTreeMap::new(),
        }
    }

    #[test]
    fn summarize_counts_total_and_by_status() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-03T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let items = vec![
            item("In Progress", Some("2026-06-01T00:00:00Z")), // 2 days → recent
            item("Todo", Some("2026-05-01T00:00:00Z")),        // >7 days → not
            item("In Progress", None),                          // no timestamp
        ];
        let s = summarize_at(&items, now);
        assert_eq!(s.total, 3);
        assert_eq!(s.by_status.get("In Progress"), Some(&2));
        assert_eq!(s.by_status.get("Todo"), Some(&1));
        assert_eq!(s.updated_recently, Some(1));
        assert_eq!(s.overdue, None);
    }
}
