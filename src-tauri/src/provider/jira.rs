//! Jira Cloud provider — REST + JQL.
//!
//! Auth is HTTP Basic with `email:api_token`: the connection's `account` holds
//! the email, the keyring token holds the API token. Issues come from the
//! enhanced search endpoint `POST /rest/api/3/search/jql`. v1 implements
//! `kind: tasks`; the brief's `query` overrides the default JQL when present.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, IntegrationItem};

/// Open issues assigned to the caller, freshest first.
const DEFAULT_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

/// Resolve `(base_url, email)` from the connection, erroring with a clear hint
/// when either is missing — both are required for Jira.
fn config(conn: &Connection) -> Result<(String, &str), String> {
    let base = conn
        .base_url
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .ok_or("Jira connection needs a base URL (e.g. https://acme.atlassian.net).")?
        .to_string();
    let email = conn
        .account
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("Jira connection needs an account email.")?;
    Ok((base, email))
}

pub async fn fetch(
    conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    if sel.kind != "tasks" {
        return Err(format!("Jira supports kind: tasks (got \"{}\").", sel.kind));
    }
    let (base, email) = config(conn)?;
    let jql = sel
        .query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_JQL);
    let max = sel.limit.unwrap_or(20).clamp(1, 100);

    let body = serde_json::json!({
        "jql": jql,
        "maxResults": max,
        "fields": ["summary", "status", "assignee", "updated", "priority", "project"],
    });
    let resp = super::http_client()?
        .post(format!("{base}/rest/api/3/search/jql"))
        .basic_auth(email, Some(token))
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let json = super::read_json(resp, "Jira").await?;
    let issues = json
        .get("issues")
        .and_then(|v| v.as_array())
        .ok_or("unexpected Jira response (no issues)")?;
    Ok(issues.iter().map(|i| map_issue(i, &base)).collect())
}

pub async fn validate(conn: &Connection, token: &str) -> Result<(), String> {
    let (base, email) = config(conn)?;
    let resp = super::http_client()?
        .get(format!("{base}/rest/api/3/myself"))
        .basic_auth(email, Some(token))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    super::read_json(resp, "Jira").await.map(|_| ())
}

/// Map one Jira issue (`{ id, key, fields: { … } }`) into a normalized item.
/// `base` builds the human `/browse/{key}` link. Pure — fixture-tested.
fn map_issue(issue: &serde_json::Value, base: &str) -> IntegrationItem {
    let key = issue.get("key").and_then(|v| v.as_str()).unwrap_or_default();
    let fields = issue.get("fields");
    let field_str = |p: &str| fields.and_then(|f| f.pointer(p)).and_then(|v| v.as_str());

    let summary = field_str("/summary").unwrap_or_default();
    let title = if key.is_empty() {
        summary.to_string()
    } else {
        format!("{key} · {summary}")
    };

    let mut meta = BTreeMap::new();
    if let Some(p) = field_str("/priority/name") {
        if !p.is_empty() {
            meta.insert("priority".to_string(), p.to_string());
        }
    }
    if let Some(p) = field_str("/project/name") {
        meta.insert("project".to_string(), p.to_string());
    }

    IntegrationItem {
        id: issue
            .get("id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| key.to_string()),
        title,
        url: if key.is_empty() {
            String::new()
        } else {
            format!("{base}/browse/{key}")
        },
        status: field_str("/status/name").map(String::from),
        assignee: field_str("/assignee/displayName").map(String::from),
        updated_at: field_str("/updated").map(String::from),
        kind: "task".to_string(),
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_jira_issue() {
        let issue: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "10042",
                "key": "PLAT-7",
                "fields": {
                    "summary": "Ship the thing",
                    "status": { "name": "In Progress" },
                    "assignee": { "displayName": "Jelani John" },
                    "updated": "2026-06-01T10:00:00.000+0000",
                    "priority": { "name": "High" },
                    "project": { "name": "Platform" }
                }
            }"#,
        )
        .unwrap();

        let item = map_issue(&issue, "https://acme.atlassian.net");
        assert_eq!(item.id, "10042");
        assert_eq!(item.title, "PLAT-7 · Ship the thing");
        assert_eq!(item.url, "https://acme.atlassian.net/browse/PLAT-7");
        assert_eq!(item.status.as_deref(), Some("In Progress"));
        assert_eq!(item.assignee.as_deref(), Some("Jelani John"));
        assert_eq!(item.meta.get("priority").map(String::as_str), Some("High"));
        assert_eq!(item.meta.get("project").map(String::as_str), Some("Platform"));
    }

    #[test]
    fn map_jira_issue_tolerates_missing_fields() {
        let issue: serde_json::Value =
            serde_json::from_str(r#"{ "key": "X-1", "fields": { "summary": "bare" } }"#).unwrap();
        let item = map_issue(&issue, "https://acme.atlassian.net/");
        assert_eq!(item.title, "X-1 · bare");
        // id falls back to the key; status/assignee absent; meta empty.
        assert_eq!(item.id, "X-1");
        assert_eq!(item.status, None);
        assert_eq!(item.assignee, None);
        assert!(item.meta.is_empty());
    }
}
