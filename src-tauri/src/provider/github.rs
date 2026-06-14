//! GitHub provider — REST. `kind: tasks` lists open issues & PRs assigned to
//! the authenticated user across all repos; `kind: notifications` lists unread
//! notifications. Auth is `Bearer <token>` (a PAT). A connection `base_url`
//! switches to a GitHub Enterprise instance (`{base_url}/api/v3`).

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, IntegrationItem};

const DEFAULT_API: &str = "https://api.github.com";

/// REST API base for the connection: api.github.com, or `{base}/api/v3` for
/// Enterprise when `base_url` is set. A `base_url` that points at public GitHub
/// (a common mix-up — the field is Enterprise-only) is ignored rather than
/// turned into a `…/api/v3` URL that 404s every call.
fn api_base(conn: &Connection) -> String {
    conn.base_url
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .filter(|s| !is_public_github(s))
        .map(|b| format!("{b}/api/v3"))
        .unwrap_or_else(|| DEFAULT_API.to_string())
}

/// Whether a `base_url` is really public github.com (or its API host) — in which
/// case there's no Enterprise instance and the default API base applies.
fn is_public_github(base: &str) -> bool {
    let host = base
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        host.as_str(),
        "github.com" | "www.github.com" | "api.github.com"
    )
}

pub async fn fetch(
    conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    let client = super::http_client()?;
    let base = api_base(conn);
    let max = sel.limit.unwrap_or(20).clamp(1, 100);

    match sel.kind.as_str() {
        "tasks" => {
            let url = format!("{base}/issues?filter=assigned&state=open&per_page={max}");
            let json = get(&client, &url, token).await?;
            let arr = json
                .as_array()
                .ok_or("unexpected GitHub response (expected an array)")?;
            Ok(arr.iter().map(map_issue).collect())
        }
        "notifications" => {
            let url = format!("{base}/notifications?per_page={max}");
            let resp = send(&client, &url, token).await?;
            // Fine-grained PATs can't reach this endpoint at all (403); steer the
            // user to a classic token rather than the generic "bad credentials".
            if resp.status() == reqwest::StatusCode::FORBIDDEN {
                return Err("GitHub notifications need a classic personal access token \
                    with the `notifications` scope — fine-grained tokens can't access \
                    this endpoint."
                    .to_string());
            }
            let json = super::read_json(resp, "GitHub").await?;
            let arr = json
                .as_array()
                .ok_or("unexpected GitHub response (expected an array)")?;
            Ok(arr.iter().map(map_notification).collect())
        }
        other => Err(format!(
            "GitHub supports kind: tasks | notifications (got \"{other}\")."
        )),
    }
}

pub async fn validate(conn: &Connection, token: &str) -> Result<(), String> {
    let client = super::http_client()?;
    get(&client, &format!("{}/user", api_base(conn)), token)
        .await
        .map(|_| ())
}

/// GET a GitHub API URL with the required headers + bearer token, returning the
/// raw response so callers can inspect the status before parsing.
async fn send(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<reqwest::Response, String> {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "WAID")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))
}

/// GET and parse a GitHub API URL as JSON, mapping non-2xx via `read_json`.
async fn get(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<serde_json::Value, String> {
    let resp = send(client, url, token).await?;
    super::read_json(resp, "GitHub").await
}

/// `https://api.github.com/repos/{owner}/{repo}…` → `{owner}/{repo}`.
fn repo_from_api_url(api_url: &str) -> Option<String> {
    let after = api_url.split("/repos/").nth(1)?;
    let mut segs = after.split('/').filter(|s| !s.is_empty());
    let owner = segs.next()?;
    let repo = segs.next()?;
    Some(format!("{owner}/{repo}"))
}

/// Map an assigned issue/PR into a normalized task. Pure — fixture-tested.
fn map_issue(issue: &serde_json::Value) -> IntegrationItem {
    let is_pr = issue.get("pull_request").is_some();
    let mut meta = BTreeMap::new();
    if let Some(repo) = issue
        .get("repository_url")
        .and_then(|v| v.as_str())
        .and_then(repo_from_api_url)
    {
        meta.insert("repo".to_string(), repo);
    }
    meta.insert(
        "type".to_string(),
        if is_pr { "PR" } else { "Issue" }.to_string(),
    );

    IntegrationItem {
        id: issue
            .get("id")
            .map(|v| v.to_string())
            .unwrap_or_default(),
        title: issue
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        url: issue
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        status: issue.get("state").and_then(|v| v.as_str()).map(String::from),
        assignee: issue
            .pointer("/assignee/login")
            .and_then(|v| v.as_str())
            .map(String::from),
        updated_at: issue.get("updated_at").and_then(|v| v.as_str()).map(String::from),
        kind: "task".to_string(),
        meta,
    }
}

/// Best-effort turn a notification subject's API URL into a browser URL.
fn web_url_from_api(api_url: &str) -> String {
    api_url
        .replace("https://api.github.com/repos/", "https://github.com/")
        .replace("/pulls/", "/pull/")
}

/// Map a notification into a normalized notification item. Pure — fixture-tested.
fn map_notification(n: &serde_json::Value) -> IntegrationItem {
    let mut meta = BTreeMap::new();
    if let Some(repo) = n.pointer("/repository/full_name").and_then(|v| v.as_str()) {
        meta.insert("repo".to_string(), repo.to_string());
    }
    if let Some(reason) = n.get("reason").and_then(|v| v.as_str()) {
        meta.insert("reason".to_string(), reason.to_string());
    }

    // Prefer a derived web URL; fall back to the repo page.
    let url = n
        .pointer("/subject/url")
        .and_then(|v| v.as_str())
        .map(web_url_from_api)
        .or_else(|| {
            n.pointer("/repository/html_url")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();

    IntegrationItem {
        id: n.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        title: n
            .pointer("/subject/title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        url,
        // The subject type (PullRequest / Issue / …) reads well as a status chip.
        status: n.pointer("/subject/type").and_then(|v| v.as_str()).map(String::from),
        assignee: None,
        updated_at: n.get("updated_at").and_then(|v| v.as_str()).map(String::from),
        kind: "notification".to_string(),
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Provider;

    fn conn_with_base(base: Option<&str>) -> Connection {
        Connection {
            id: "gh".into(),
            provider: Provider::Github,
            label: "GitHub".into(),
            base_url: base.map(String::from),
            account: None,
        }
    }

    #[test]
    fn api_base_defaults_to_public_api() {
        assert_eq!(api_base(&conn_with_base(None)), DEFAULT_API);
        assert_eq!(api_base(&conn_with_base(Some("   "))), DEFAULT_API);
    }

    #[test]
    fn api_base_ignores_public_github_in_base_url() {
        // The field is Enterprise-only; a public-GitHub value must not become
        // a `…/api/v3` URL (which 404s every call).
        for b in [
            "https://github.com",
            "https://github.com/",
            "https://api.github.com",
            "http://www.github.com",
            "github.com/jelanijohn/waid",
        ] {
            assert_eq!(api_base(&conn_with_base(Some(b))), DEFAULT_API, "base={b}");
        }
    }

    #[test]
    fn api_base_builds_enterprise_path() {
        assert_eq!(
            api_base(&conn_with_base(Some("https://github.example.com"))),
            "https://github.example.com/api/v3"
        );
    }

    #[test]
    fn maps_assigned_pr() {
        let issue: serde_json::Value = serde_json::from_str(
            r#"{
                "id": 9876,
                "title": "Add provider dispatch",
                "html_url": "https://github.com/acme/waid/pull/12",
                "state": "open",
                "updated_at": "2026-06-01T10:00:00Z",
                "assignee": { "login": "jelani" },
                "repository_url": "https://api.github.com/repos/acme/waid",
                "pull_request": { "url": "https://api.github.com/repos/acme/waid/pulls/12" }
            }"#,
        )
        .unwrap();

        let item = map_issue(&issue);
        assert_eq!(item.id, "9876");
        assert_eq!(item.title, "Add provider dispatch");
        assert_eq!(item.url, "https://github.com/acme/waid/pull/12");
        assert_eq!(item.status.as_deref(), Some("open"));
        assert_eq!(item.assignee.as_deref(), Some("jelani"));
        assert_eq!(item.meta.get("repo").map(String::as_str), Some("acme/waid"));
        assert_eq!(item.meta.get("type").map(String::as_str), Some("PR"));
    }

    #[test]
    fn maps_plain_issue_as_issue_type() {
        let issue: serde_json::Value = serde_json::from_str(
            r#"{ "id": 1, "title": "bug", "html_url": "https://github.com/acme/waid/issues/3",
                 "state": "open", "repository_url": "https://api.github.com/repos/acme/waid" }"#,
        )
        .unwrap();
        let item = map_issue(&issue);
        assert_eq!(item.meta.get("type").map(String::as_str), Some("Issue"));
        assert_eq!(item.assignee, None);
    }

    #[test]
    fn maps_notification_and_derives_web_url() {
        let n: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "n1",
                "reason": "review_requested",
                "updated_at": "2026-06-02T08:00:00Z",
                "subject": {
                    "title": "Tighten the timeout",
                    "type": "PullRequest",
                    "url": "https://api.github.com/repos/acme/waid/pulls/42"
                },
                "repository": { "full_name": "acme/waid", "html_url": "https://github.com/acme/waid" }
            }"#,
        )
        .unwrap();

        let item = map_notification(&n);
        assert_eq!(item.id, "n1");
        assert_eq!(item.kind, "notification");
        assert_eq!(item.title, "Tighten the timeout");
        assert_eq!(item.url, "https://github.com/acme/waid/pull/42");
        assert_eq!(item.status.as_deref(), Some("PullRequest"));
        assert_eq!(item.meta.get("repo").map(String::as_str), Some("acme/waid"));
        assert_eq!(item.meta.get("reason").map(String::as_str), Some("review_requested"));
    }
}
