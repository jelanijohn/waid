//! GitHub provider — REST. Feed kinds:
//! - `tasks` — open issues & PRs assigned to the authenticated user across all
//!   repos (`/issues?filter=assigned`).
//! - `notifications` — unread notifications.
//! - `pulls` — pull requests matching a search `query` (`/search/issues`,
//!   defaulting to `is:pr is:open author:@me`); reuses the issue mapper.
//! - `commits` — commits matching a search `query` (`/search/commits`,
//!   defaulting to `author:@me`).
//!
//! `pulls` and `commits` are independent feeds, not a toggle: a brief can carry
//! either, both, or neither. They only overlap once a PR merges onto a branch
//! the commits query covers — in-flight PR commits live on the feature branch.
//!
//! **Repo scoping (required).** The search feeds (`pulls`, `commits`) hit
//! `/search/...`, which returns *public* matches regardless of the token's repo
//! grant — so a bare `author:@me` would surface a user's commits across every
//! repo they've ever touched, not just this project's. A token's scope can't
//! prevent this (public search ignores it), so WAID requires every feed to name
//! a repo: the connection's `repos` (`owner/name`, one or more). There is **no**
//! fall-back to "all the token's repos" — an unscoped feed is rejected, never
//! silently broadened. Search feeds get `repo:` qualifiers injected from `repos`
//! (unless the query already pins scope with a `repo:`/`org:`/`user:` qualifier,
//! an explicit power-user opt-in); the REST feeds (`tasks`, `notifications`) are
//! filtered down to `repos`.
//!
//! Auth is `Bearer <token>` (a PAT). A connection `base_url` switches to a
//! GitHub Enterprise instance (`{base_url}/api/v3`).

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
    // Every GitHub feed must name its repo(s) — there is no global fallback.
    let override_repos = repo_override(conn);

    match sel.kind.as_str() {
        "tasks" => {
            let repos = require_repos(&override_repos)?;
            let url = format!("{base}/issues?filter=assigned&state=open&per_page={max}");
            let json = get(&client, &url, token).await?;
            let arr = json
                .as_array()
                .ok_or("unexpected GitHub response (expected an array)")?;
            let mut items: Vec<IntegrationItem> = arr.iter().map(map_issue).collect();
            // The assigned-issues endpoint is cross-repo; keep only this project's.
            retain_by_repos(&mut items, repos);
            Ok(items)
        }
        "notifications" => {
            let repos = require_repos(&override_repos)?;
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
            let mut items: Vec<IntegrationItem> = arr.iter().map(map_notification).collect();
            retain_by_repos(&mut items, repos);
            Ok(items)
        }
        "pulls" => {
            // Search-issues items share the issue shape (html_url, state,
            // repository_url, pull_request, assignee), so map_issue applies.
            let q = resolve_scope(sel, "is:pr is:open author:@me", &override_repos)?;
            let json = search(&client, &base, "issues", &q, max, token).await?;
            Ok(search_items(&json)?.iter().map(map_issue).collect())
        }
        "commits" => {
            let q = resolve_scope(sel, "author:@me", &override_repos)?;
            let json = search(&client, &base, "commits", &q, max, token).await?;
            Ok(search_items(&json)?.iter().map(map_commit).collect())
        }
        other => Err(format!(
            "GitHub supports kind: tasks | notifications | pulls | commits (got \"{other}\")."
        )),
    }
}

/// Cap on how many `repo:` qualifiers we splice into a single search query.
/// GitHub caps the `q` length, so a connection naming a long `repos` list can't
/// have every one injected — we take the first (the intended setup is one repo,
/// or a small handful, where this never bites).
const MAX_INJECTED_REPOS: usize = 10;

/// The error shown when a GitHub feed has no repo scope. Scoping is mandatory:
/// without it, `pulls`/`commits` become global public searches across unrelated
/// projects (a token's scope can't prevent that).
const UNSCOPED_ERR: &str = "This GitHub feed isn't scoped to a repository. Add one or more \
    repos (owner/name) to the connection so it only pulls from your project — never a broad \
    search across every repo you've touched.";

/// Require an explicit repo scope, erroring (rather than searching broadly) when
/// the connection names none. Used by the REST feeds, whose only scope source is
/// the connection's `repos`.
fn require_repos(override_repos: &Option<Vec<String>>) -> Result<&[String], String> {
    override_repos
        .as_deref()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| UNSCOPED_ERR.to_string())
}

/// Build the search query for a `pulls`/`commits` feed, scoped so it can't
/// return matches from unrelated repos. Pure — no network.
///
/// If the selector's query already pins scope with a `repo:`/`org:`/`user:`
/// qualifier, it's honored verbatim (an explicit power-user opt-in). Otherwise
/// the connection's `repos` are injected as leading `repo:` qualifiers; with
/// neither, the feed is rejected rather than run as a global search.
fn resolve_scope(
    sel: &BriefIntegration,
    default: &str,
    override_repos: &Option<Vec<String>>,
) -> Result<String, String> {
    let base_q = query_or(sel, default);
    if has_scope_qualifier(&base_q) {
        return Ok(base_q);
    }
    let repos = require_repos(override_repos)?;
    Ok(prepend_repo_qualifiers(&base_q, repos))
}

/// The connection's explicit `repos`, normalized to `owner/name` and dropped if
/// empty. `None` means "unscoped" — which the feeds reject (scoping is required).
fn repo_override(conn: &Connection) -> Option<Vec<String>> {
    let normalized: Vec<String> = conn
        .repos
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|r| normalize_repo(r))
        .collect();
    (!normalized.is_empty()).then_some(normalized)
}

/// Normalize a user-entered repo to `owner/name`, tolerating a pasted URL
/// (`https://github.com/owner/name`), a trailing `.git`, and stray slashes.
/// Returns `None` if it isn't at least `owner/name`.
fn normalize_repo(raw: &str) -> Option<String> {
    let s = raw.trim();
    // Drop everything up to and including a `github.com/` host if a URL was pasted.
    let s = s.rsplit("github.com/").next().unwrap_or(s);
    let s = s.trim_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let mut segs = s.split('/').filter(|p| !p.is_empty());
    let owner = segs.next()?;
    let name = segs.next()?;
    Some(format!("{owner}/{name}"))
}

/// Whether a search query already pins its scope to a repo/org/user, in which
/// case we leave it untouched rather than injecting our own `repo:` qualifiers.
fn has_scope_qualifier(q: &str) -> bool {
    q.split_whitespace().any(|tok| {
        let t = tok.to_ascii_lowercase();
        t.starts_with("repo:") || t.starts_with("org:") || t.starts_with("user:")
    })
}

/// Prefix a query with `repo:owner/name` qualifiers (capped at
/// `MAX_INJECTED_REPOS`). Multiple `repo:` qualifiers are OR'd by GitHub search.
fn prepend_repo_qualifiers(q: &str, repos: &[String]) -> String {
    let quals = repos
        .iter()
        .take(MAX_INJECTED_REPOS)
        .map(|r| format!("repo:{r}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{quals} {q}").trim().to_string()
}

/// Keep only items whose `repo` meta is in the allow-set (case-insensitive).
/// Items without a `repo` meta are dropped — we can't prove they're in scope.
fn retain_by_repos(items: &mut Vec<IntegrationItem>, repos: &[String]) {
    let allow: Vec<String> = repos.iter().map(|r| r.to_ascii_lowercase()).collect();
    items.retain(|it| {
        it.meta
            .get("repo")
            .map(|r| allow.contains(&r.to_ascii_lowercase()))
            .unwrap_or(false)
    });
}

/// The selector's trimmed non-empty `query`, or `default` when omitted/blank.
/// Unlike Slack/Gmail, GitHub search feeds have a sensible `@me` default, so the
/// query is optional — it scopes/overrides rather than being required.
fn query_or(sel: &BriefIntegration, default: &str) -> String {
    sel.query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

/// GET a `/search/{path}` endpoint (`issues` | `commits`), letting reqwest
/// URL-encode the query string. Returns the parsed `{ "items": [...] }` body.
async fn search(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    q: &str,
    max: u32,
    token: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .get(format!("{base}/search/{path}"))
        .header(reqwest::header::USER_AGENT, "WAID")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .bearer_auth(token)
        .query(&[("q", q), ("per_page", &max.to_string())])
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    super::read_json(resp, "GitHub").await
}

/// The `items` array of a search response (search endpoints wrap results in an
/// object, unlike the bare arrays from `/issues` and `/notifications`).
fn search_items(json: &serde_json::Value) -> Result<&Vec<serde_json::Value>, String> {
    json.get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "unexpected GitHub response (expected search results)".to_string())
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

/// Map a `/search/commits` item into a normalized item. Pure — fixture-tested.
/// Title is the commit subject (first message line); the short SHA and repo go
/// into meta, and the GitHub author login rides the `assignee` slot (the "who").
fn map_commit(commit: &serde_json::Value) -> IntegrationItem {
    let sha = commit.get("sha").and_then(|v| v.as_str()).unwrap_or_default();
    let message = commit
        .pointer("/commit/message")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let mut meta = BTreeMap::new();
    if let Some(repo) = commit.pointer("/repository/full_name").and_then(|v| v.as_str()) {
        meta.insert("repo".to_string(), repo.to_string());
    }
    if !sha.is_empty() {
        meta.insert("sha".to_string(), sha.chars().take(7).collect());
    }
    meta.insert("type".to_string(), "Commit".to_string());

    IntegrationItem {
        id: sha.to_string(),
        title: message.lines().next().unwrap_or_default().trim().to_string(),
        url: commit.get("html_url").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        status: None,
        // The committing GitHub user (may be null for unmatched email authors).
        assignee: commit.pointer("/author/login").and_then(|v| v.as_str()).map(String::from),
        updated_at: commit.pointer("/commit/author/date").and_then(|v| v.as_str()).map(String::from),
        kind: "commit".to_string(),
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
            repos: None,
            ws_url: None,
            data_dir: None,
            token_path: None,
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

    #[test]
    fn maps_commit_subject_and_short_sha() {
        let commit: serde_json::Value = serde_json::from_str(
            r#"{
                "sha": "abc1234def5678",
                "html_url": "https://github.com/acme/waid/commit/abc1234def5678",
                "commit": {
                    "message": "Add commits feed\n\nLonger body that must be dropped.",
                    "author": { "name": "Jelani", "date": "2026-06-10T09:30:00Z" }
                },
                "author": { "login": "jelanijohn" },
                "repository": { "full_name": "acme/waid" }
            }"#,
        )
        .unwrap();

        let item = map_commit(&commit);
        assert_eq!(item.kind, "commit");
        assert_eq!(item.id, "abc1234def5678");
        assert_eq!(item.title, "Add commits feed");
        assert_eq!(item.url, "https://github.com/acme/waid/commit/abc1234def5678");
        assert_eq!(item.status, None);
        assert_eq!(item.assignee.as_deref(), Some("jelanijohn"));
        assert_eq!(item.updated_at.as_deref(), Some("2026-06-10T09:30:00Z"));
        assert_eq!(item.meta.get("repo").map(String::as_str), Some("acme/waid"));
        assert_eq!(item.meta.get("sha").map(String::as_str), Some("abc1234"));
        assert_eq!(item.meta.get("type").map(String::as_str), Some("Commit"));
    }

    #[test]
    fn map_commit_tolerates_missing_author() {
        let commit: serde_json::Value = serde_json::from_str(
            r#"{ "sha": "f00", "html_url": "https://github.com/acme/waid/commit/f00",
                 "commit": { "message": "tidy" } }"#,
        )
        .unwrap();
        let item = map_commit(&commit);
        assert_eq!(item.assignee, None);
        assert_eq!(item.title, "tidy");
        assert_eq!(item.meta.get("sha").map(String::as_str), Some("f00"));
    }

    #[test]
    fn normalize_repo_handles_urls_and_plain_names() {
        assert_eq!(normalize_repo("jelanijohn/waid").as_deref(), Some("jelanijohn/waid"));
        assert_eq!(
            normalize_repo("  https://github.com/jelanijohn/waid.git  ").as_deref(),
            Some("jelanijohn/waid")
        );
        assert_eq!(
            normalize_repo("github.com/acme/waid/").as_deref(),
            Some("acme/waid")
        );
        // Not enough to form owner/name.
        assert_eq!(normalize_repo("waid"), None);
        assert_eq!(normalize_repo("   "), None);
    }

    #[test]
    fn repo_override_normalizes_and_drops_empty() {
        let mut conn = conn_with_base(None);
        conn.repos = Some(vec!["jelanijohn/waid".into(), "  ".into(), "not-a-repo".into()]);
        assert_eq!(repo_override(&conn), Some(vec!["jelanijohn/waid".to_string()]));

        conn.repos = Some(vec![]);
        assert_eq!(repo_override(&conn), None);

        conn.repos = None;
        assert_eq!(repo_override(&conn), None);
    }

    #[test]
    fn has_scope_qualifier_detects_repo_org_user() {
        assert!(has_scope_qualifier("repo:acme/waid author:@me"));
        assert!(has_scope_qualifier("is:pr ORG:acme")); // case-insensitive
        assert!(has_scope_qualifier("user:jelanijohn"));
        assert!(!has_scope_qualifier("author:@me is:open"));
        assert!(!has_scope_qualifier("is:pr is:open author:@me"));
    }

    #[test]
    fn prepend_repo_qualifiers_caps_and_orders() {
        let q = prepend_repo_qualifiers("author:@me", &["a/one".into(), "a/two".into()]);
        assert_eq!(q, "repo:a/one repo:a/two author:@me");

        // Capped at MAX_INJECTED_REPOS, preserving order.
        let many: Vec<String> = (0..15).map(|i| format!("a/r{i}")).collect();
        let q = prepend_repo_qualifiers("author:@me", &many);
        let injected = q.matches("repo:").count();
        assert_eq!(injected, MAX_INJECTED_REPOS);
        assert!(q.starts_with("repo:a/r0 "));
        assert!(q.ends_with(" author:@me"));
    }

    fn sel(kind: &str, query: Option<&str>) -> BriefIntegration {
        BriefIntegration {
            connection: "gh".into(),
            kind: kind.into(),
            query: query.map(String::from),
            limit: None,
        }
    }

    #[test]
    fn resolve_scope_injects_repos_from_override() {
        let repos = Some(vec!["acme/waid".to_string()]);
        let q = resolve_scope(&sel("commits", None), "author:@me", &repos).unwrap();
        assert_eq!(q, "repo:acme/waid author:@me");
    }

    #[test]
    fn resolve_scope_honors_explicit_query_qualifier() {
        // An explicit org:/repo:/user: query is a power-user opt-in — left as-is,
        // and allowed even with no `repos` override.
        let q = resolve_scope(&sel("commits", Some("org:acme author:@me")), "author:@me", &None)
            .unwrap();
        assert_eq!(q, "org:acme author:@me");
    }

    #[test]
    fn resolve_scope_rejects_unscoped_feed() {
        // No override and no scope qualifier → refuse rather than search globally.
        let err = resolve_scope(&sel("commits", None), "author:@me", &None).unwrap_err();
        assert!(err.contains("isn't scoped"), "got: {err}");

        // An empty override list counts as unscoped too.
        let err = resolve_scope(&sel("commits", None), "author:@me", &Some(vec![])).unwrap_err();
        assert!(err.contains("isn't scoped"), "got: {err}");
    }

    #[test]
    fn require_repos_demands_a_scope() {
        assert!(require_repos(&None).is_err());
        assert!(require_repos(&Some(vec![])).is_err());
        assert_eq!(
            require_repos(&Some(vec!["acme/waid".into()])).unwrap(),
            &["acme/waid".to_string()][..]
        );
    }

    #[test]
    fn retain_by_repos_filters_case_insensitively() {
        let mk = |repo: Option<&str>| {
            let mut meta = BTreeMap::new();
            if let Some(r) = repo {
                meta.insert("repo".to_string(), r.to_string());
            }
            IntegrationItem {
                id: "x".into(),
                title: "t".into(),
                url: "u".into(),
                status: None,
                assignee: None,
                updated_at: None,
                kind: "task".into(),
                meta,
            }
        };
        let mut items = vec![mk(Some("acme/Waid")), mk(Some("other/repo")), mk(None)];
        retain_by_repos(&mut items, &["acme/waid".into()]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].meta.get("repo").map(String::as_str), Some("acme/Waid"));
    }
}
