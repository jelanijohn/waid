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
pub mod figma;
pub mod github;
pub mod gmail;
pub mod jira;
pub mod linear;
pub mod notion;
pub mod slack;

/// The supported providers. Serialized lowercase (`"linear"`) to mirror the
/// `Provider` string union in `types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Linear,
    Jira,
    Asana,
    Github,
    Notion,
    Gmail,
    Slack,
    Figma,
    /// Local EEG dashboard (NeuroSkill). Unlike every other variant this is **not**
    /// a panel feed and is **not** dispatched through this module's network fetch —
    /// it reads local SQLite and writes the `## Mind State` body region via the
    /// `crate::neuroskill` module / `sync_mind_state`. It lives in the enum only so
    /// connections parse from frontmatter and share the connection/selector
    /// plumbing. No token, no keyring entry (localhost, no auth).
    Neuroskill,
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

// --- Typed fetch errors + server-reported limits ---------------------------
//
// `fetch_integration` rejects with a `FetchError` so the auto-sync scheduler can
// tell a rate limit (and how long to wait) from any other failure. Every `?` on
// a `String` helper still compiles through `From<String>` (kind `Other`).

/// What went wrong, for the scheduler. Serialized flat next to `message`:
/// `{"message": "…", "kind": "rateLimited", "retryAfterMs": 30000}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FetchErrorKind {
    #[serde(rename_all = "camelCase")]
    RateLimited { retry_after_ms: Option<u64> },
    Auth,
    Http { status: u16 },
    Network,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchError {
    pub message: String,
    #[serde(flatten)]
    pub kind: FetchErrorKind,
}

impl FetchError {
    pub fn rate_limited(message: impl Into<String>, retry_after_ms: Option<u64>) -> Self {
        Self {
            message: message.into(),
            kind: FetchErrorKind::RateLimited { retry_after_ms },
        }
    }

    /// A failed `send()` (DNS, connect, timeout) — never reached the provider.
    pub fn network(e: reqwest::Error) -> Self {
        Self {
            message: format!("request failed: {e}"),
            kind: FetchErrorKind::Network,
        }
    }

    /// Fill a missing `Retry-After` on a rate-limit error from the response
    /// headers (Slack reports the limit in-band but the wait in a header).
    pub fn with_retry_after(mut self, ms: Option<u64>) -> Self {
        if let FetchErrorKind::RateLimited { retry_after_ms } = &mut self.kind {
            if retry_after_ms.is_none() {
                *retry_after_ms = ms;
            }
        }
        self
    }
}

impl From<String> for FetchError {
    fn from(message: String) -> Self {
        Self {
            message,
            kind: FetchErrorKind::Other,
        }
    }
}

impl From<&str> for FetchError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// HTTP cache validators from a response, replayed as `If-None-Match` /
/// `If-Modified-Since` on the next poll of the same feed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Validators {
    fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }
}

/// Rate-limit state the provider reported on its last response.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u32>,
    /// RFC3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
    /// Server-requested minimum poll interval (GitHub `X-Poll-Interval`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_interval_ms: Option<u64>,
}

/// Everything the scheduler and the cache want from a response's headers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResponseMeta {
    pub status: u16,
    pub validators: Validators,
    pub rate: Option<RateInfo>,
    pub retry_after_ms: Option<u64>,
}

impl ResponseMeta {
    /// Parse `ETag`, `Last-Modified`, `X-RateLimit-Remaining/Reset` (GitHub) and
    /// `X-RateLimit-Requests-Remaining/Reset` (Linear), `Retry-After` (seconds or
    /// HTTP-date) and `X-Poll-Interval`. Pure — `now` is injected.
    pub fn from_headers(
        status: u16,
        headers: &reqwest::header::HeaderMap,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let h = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        let validators = Validators {
            etag: h("etag"),
            last_modified: h("last-modified"),
        };

        let remaining = h("x-ratelimit-remaining")
            .or_else(|| h("x-ratelimit-requests-remaining"))
            .and_then(|s| s.parse::<u32>().ok());
        let reset_at = h("x-ratelimit-reset")
            .or_else(|| h("x-ratelimit-requests-reset"))
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(|n| {
                // GitHub sends epoch seconds, Linear epoch milliseconds.
                let ms = if n > 100_000_000_000 { n } else { n * 1000 };
                chrono::DateTime::from_timestamp_millis(ms)
            })
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        let min_interval_ms = h("x-poll-interval")
            .and_then(|s| s.parse::<u64>().ok())
            .map(|s| s * 1000);
        let rate = (remaining.is_some() || reset_at.is_some() || min_interval_ms.is_some())
            .then(|| RateInfo {
                remaining,
                reset_at,
                min_interval_ms,
            });

        let retry_after_ms = h("retry-after").and_then(|s| {
            if let Ok(secs) = s.parse::<u64>() {
                return Some(secs * 1000);
            }
            let at = chrono::DateTime::parse_from_rfc2822(&s).ok()?;
            Some((at.with_timezone(&chrono::Utc) - now).num_milliseconds().max(0) as u64)
        });

        Self {
            status,
            validators,
            rate,
            retry_after_ms,
        }
    }

    fn validators(&self) -> Option<Validators> {
        (!self.validators.is_empty()).then(|| self.validators.clone())
    }
}

/// A read response: `body` is `None` on a 304.
pub struct Read {
    pub body: Option<serde_json::Value>,
    pub meta: ResponseMeta,
}

impl Read {
    /// The JSON body; a 304 here is a bug in the caller (no validators sent).
    pub fn json(self, provider: &str) -> Result<serde_json::Value, FetchError> {
        self.body
            .ok_or_else(|| format!("{provider} returned 304 Not Modified unexpectedly").into())
    }
}

/// Read a response, mapping auth failures, rate limits and non-2xx into clear,
/// provider-labelled, typed errors. A 304 is a success with no body.
/// GraphQL-style in-band errors (Linear) are checked by the caller.
pub(crate) async fn read_response(
    resp: reqwest::Response,
    provider: &str,
) -> Result<Read, FetchError> {
    let status = resp.status();
    let meta = ResponseMeta::from_headers(status.as_u16(), resp.headers(), chrono::Utc::now());
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Read { body: None, meta });
    }
    // GitHub signals primary/secondary limits with a 403 + Retry-After or
    // X-RateLimit-Remaining: 0, not only a 429.
    let exhausted = meta.retry_after_ms.is_some()
        || meta.rate.as_ref().and_then(|r| r.remaining) == Some(0);
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN && exhausted)
    {
        return Err(FetchError::rate_limited(
            format!("{provider} rate limit hit — WAID will wait before trying again."),
            meta.retry_after_ms,
        ));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(FetchError {
            message: format!("{provider} rejected the credentials — check the connection in Settings."),
            kind: FetchErrorKind::Auth,
        });
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(FetchError {
            message: format!(
                "{provider} returned {status}: {}",
                text.chars().take(200).collect::<String>()
            ),
            kind: FetchErrorKind::Http {
                status: status.as_u16(),
            },
        });
    }
    let body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("invalid JSON from {provider}: {e}"))?;
    Ok(Read {
        body: Some(body),
        meta,
    })
}

/// Read a JSON response as a plain `String` error — the shim for the call sites
/// (validate, evidence, lookups) that don't need typed errors or headers.
pub(crate) async fn read_json(
    resp: reqwest::Response,
    provider: &str,
) -> Result<serde_json::Value, String> {
    read_response(resp, provider)
        .await
        .and_then(|r| r.json(provider))
        .map_err(|e| e.message)
}

// --- The provider fetch seam ------------------------------------------------

/// What the command layer hands a provider besides the token: validators from
/// the last response (conditional requests) and a cached auxiliary lookup
/// (Slack users, Figma handle, Asana workspace, Notion data source).
#[derive(Default)]
pub struct FetchCtx<'a> {
    pub validators: Option<&'a Validators>,
    pub aux: Option<&'a serde_json::Value>,
}

/// A provider's result. `cost` is the true number of requests made.
pub enum FetchOutcome {
    NotModified {
        cost: u32,
        rate: Option<RateInfo>,
    },
    Fresh {
        items: Vec<IntegrationItem>,
        cost: u32,
        validators: Option<Validators>,
        rate: Option<RateInfo>,
        /// A refreshed auxiliary lookup for the cache (`None` = nothing new).
        aux: Option<serde_json::Value>,
    },
}

impl FetchOutcome {
    pub fn fresh(items: Vec<IntegrationItem>, cost: u32) -> Self {
        Self::Fresh {
            items,
            cost,
            validators: None,
            rate: None,
            aux: None,
        }
    }

    /// Attach the main response's rate info (and validators, when present).
    pub fn with_meta(mut self, meta: &ResponseMeta) -> Self {
        match &mut self {
            Self::Fresh {
                validators, rate, ..
            } => {
                *validators = meta.validators();
                *rate = meta.rate.clone();
            }
            Self::NotModified { rate, .. } => *rate = meta.rate.clone(),
        }
        self
    }

    pub fn with_aux(mut self, value: Option<serde_json::Value>) -> Self {
        if let Self::Fresh { aux, .. } = &mut self {
            *aux = value;
        }
        self
    }
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
    /// GitHub only: the `owner/name` repos this connection's feeds are scoped to.
    /// **Required** for GitHub — a feed with no repo scope is rejected rather than
    /// run as a global search, so a brief never pulls commits/PRs from unrelated
    /// projects (a token's repo grant can't prevent public search from doing so).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<Vec<String>>,
    /// NeuroSkill only: the local daemon endpoint the `label` write targets
    /// (default `http://127.0.0.1:18444`). A `ws://host:port` value is accepted
    /// for back-compat and treated as the same host/port over HTTP. Overridable
    /// for the WSL2↔Windows-host split. Not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
    /// NeuroSkill only: the directory holding `activity.sqlite` / `labels.sqlite`
    /// (default the WSL-translated AppData path). Overridable. Not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    /// NeuroSkill only: path to the daemon's bearer-token file (default the OS
    /// `…/skill/daemon/auth.token`). The daemon writes this file and gates its
    /// API on the token; on WSL2 it lives on the Windows host, so it's overridable
    /// (e.g. the `/mnt/c/.../AppData/Roaming/skill/daemon/auth.token` path). WAID
    /// reads it at call time — not stored by WAID, not a keyring secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_path: Option<String>,
}

/// A brief's reference to a connection plus a provider-specific selector. Parsed
/// from frontmatter alongside `links` / `webhooks`, and round-tripped via the
/// brief's `raw` on save — no special write handling needed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BriefIntegration {
    /// References `Connection.id`.
    pub connection: String,
    /// e.g. `"tasks"` | `"notifications"` | `"pulls"` | `"commits"` (GitHub).
    /// Provider-specific; defaults to tasks when omitted.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Provider-specific selector (e.g. a Linear query). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Optional cap on the number of items fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// `false` opts this feed out of background auto-sync (manual refresh still
    /// works). Absent means on, so existing files are untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<bool>,
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

/// Where a fetch's items came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServedFrom {
    /// A real request returned new data.
    Network,
    /// The in-memory response cache, within the caller's max age (no request).
    Cache,
    /// A conditional request answered 304; the cached items were re-served.
    NotModified,
}

/// What `fetch_integration` returns: the normalized items, when they were
/// fetched, the local rollup, and the bookkeeping the auto-sync scheduler
/// budgets with.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationFetch {
    pub items: Vec<IntegrationItem>,
    pub fetched_at: String,
    pub summary: IntegrationSummary,
    /// Session-salted hash of the credential (see `commands::credential_id`).
    /// Feeds sharing a token share an id, and so a rate-limit budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    /// Requests this fetch actually made (0 for a cache hit).
    pub cost: u32,
    pub served_from: ServedFrom,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<RateInfo>,
}

impl IntegrationFetch {
    /// Assemble a result and attach the local rollup.
    pub fn new(
        items: Vec<IntegrationItem>,
        fetched_at: String,
        credential_id: Option<String>,
        cost: u32,
        served_from: ServedFrom,
        rate: Option<RateInfo>,
    ) -> Self {
        let summary = summarize(&items);
        Self {
            items,
            fetched_at,
            summary,
            credential_id,
            cost,
            served_from,
            rate,
        }
    }
}

/// Dispatch a fetch to the right provider. The command layer owns the cache,
/// the clock and the rollup; providers only report what they fetched and what
/// it cost. Unsupported providers return a clear error rather than panic.
pub async fn fetch(
    conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
    ctx: &FetchCtx<'_>,
) -> Result<FetchOutcome, FetchError> {
    match conn.provider {
        Provider::Linear => linear::fetch(conn, sel, token).await,
        Provider::Jira => jira::fetch(conn, sel, token).await,
        Provider::Asana => asana::fetch(conn, sel, token, ctx).await,
        Provider::Github => github::fetch(conn, sel, token, ctx).await,
        Provider::Notion => notion::fetch(conn, sel, token, ctx).await,
        Provider::Gmail => gmail::fetch(conn, sel, token).await,
        Provider::Slack => slack::fetch(conn, sel, token, ctx).await,
        Provider::Figma => figma::fetch(conn, sel, token, ctx).await,
        // NeuroSkill is not a panel feed — it syncs into the `## Mind State` body
        // region via `sync_mind_state`, not through this network dispatch.
        Provider::Neuroskill => Err("NeuroSkill mind-state feeds aren't panel items — they sync into \
the ## Mind State region via sync_mind_state."
            .into()),
    }
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
        Provider::Figma => figma::validate(conn, token).await,
        // No auth to validate (localhost, no token). Real reachability of the
        // SQLite store / WebSocket daemon surfaces when the region is synced.
        Provider::Neuroskill => Ok(()),
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

    fn headers(pairs: &[(&'static str, &str)]) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    fn at(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&chrono::Utc)
    }

    #[test]
    fn response_meta_parses_github_limit_headers() {
        let h = headers(&[
            ("etag", "W/\"abc\""),
            ("last-modified", "Thu, 08 Oct 2026 12:00:00 GMT"),
            ("x-ratelimit-remaining", "4321"),
            ("x-ratelimit-reset", "1791460800"), // epoch seconds
            ("x-poll-interval", "60"),
        ]);
        let m = ResponseMeta::from_headers(200, &h, at("2026-10-09T00:00:00Z"));
        assert_eq!(m.validators.etag.as_deref(), Some("W/\"abc\""));
        assert_eq!(m.validators.last_modified.as_deref(), Some("Thu, 08 Oct 2026 12:00:00 GMT"));
        let rate = m.rate.unwrap();
        assert_eq!(rate.remaining, Some(4321));
        assert_eq!(rate.reset_at.as_deref(), Some("2026-10-08T12:00:00Z"));
        assert_eq!(rate.min_interval_ms, Some(60_000));
        assert_eq!(m.retry_after_ms, None);
    }

    #[test]
    fn response_meta_parses_linear_millisecond_reset() {
        let h = headers(&[
            ("x-ratelimit-requests-remaining", "12"),
            ("x-ratelimit-requests-reset", "1791460800000"), // epoch ms
        ]);
        let rate = ResponseMeta::from_headers(200, &h, at("2026-10-09T00:00:00Z")).rate.unwrap();
        assert_eq!(rate.remaining, Some(12));
        assert_eq!(rate.reset_at.as_deref(), Some("2026-10-08T12:00:00Z"));
    }

    #[test]
    fn response_meta_parses_retry_after_seconds_and_http_date() {
        let now = at("2026-10-09T12:00:00Z");
        let secs = ResponseMeta::from_headers(429, &headers(&[("retry-after", "30")]), now);
        assert_eq!(secs.retry_after_ms, Some(30_000));
        let date = ResponseMeta::from_headers(
            429,
            &headers(&[("retry-after", "Fri, 09 Oct 2026 12:02:00 GMT")]),
            now,
        );
        assert_eq!(date.retry_after_ms, Some(120_000));
        // A date in the past means "now", not a negative wait.
        let past = ResponseMeta::from_headers(
            429,
            &headers(&[("retry-after", "Fri, 09 Oct 2026 11:00:00 GMT")]),
            now,
        );
        assert_eq!(past.retry_after_ms, Some(0));
    }

    #[test]
    fn response_meta_without_headers_is_empty() {
        let m = ResponseMeta::from_headers(304, &headers(&[]), at("2026-10-09T00:00:00Z"));
        assert_eq!(m.status, 304);
        assert!(m.rate.is_none());
        assert!(m.validators().is_none());
    }

    #[test]
    fn not_modified_read_has_no_body() {
        let r = Read {
            body: None,
            meta: ResponseMeta::default(),
        };
        assert!(r.json("GitHub").is_err());
    }

    #[test]
    fn fetch_error_serializes_flat_and_tagged() {
        let e = FetchError::rate_limited("slow down", Some(30_000));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"message": "slow down", "kind": "rateLimited", "retryAfterMs": 30000})
        );
        let http = FetchError {
            message: "nope".into(),
            kind: FetchErrorKind::Http { status: 500 },
        };
        assert_eq!(
            serde_json::to_value(&http).unwrap(),
            serde_json::json!({"message": "nope", "kind": "http", "status": 500})
        );
    }

    #[test]
    fn fetch_error_from_string_is_other() {
        let e: FetchError = String::from("boom").into();
        assert_eq!(e.kind, FetchErrorKind::Other);
        assert_eq!(e.to_string(), "boom");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"message": "boom", "kind": "other"})
        );
    }

    #[test]
    fn with_retry_after_fills_only_missing_waits() {
        let e = FetchError::rate_limited("x", None).with_retry_after(Some(5_000));
        assert_eq!(e.kind, FetchErrorKind::RateLimited { retry_after_ms: Some(5_000) });
        let kept = FetchError::rate_limited("x", Some(1_000)).with_retry_after(Some(5_000));
        assert_eq!(kept.kind, FetchErrorKind::RateLimited { retry_after_ms: Some(1_000) });
        let other = FetchError::from("x").with_retry_after(Some(5_000));
        assert_eq!(other.kind, FetchErrorKind::Other);
    }
}
