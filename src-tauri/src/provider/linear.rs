//! Linear provider — a single JSON POST to the GraphQL endpoint.
//!
//! Auth is the personal API key sent verbatim in the `Authorization` header
//! (Linear API keys are used directly, *not* as `Bearer <token>` — that form is
//! only for OAuth access tokens). v1 implements `kind: tasks` as "issues assigned
//! to me that are still open", newest first. The brief's `query` selector is
//! reserved for a later phase and is deliberately not interpreted here, so a
//! stray selector can't silently filter everything out.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, FetchError, FetchOutcome, IntegrationItem};

const ENDPOINT: &str = "https://api.linear.app/graphql";

/// Issues assigned to the authenticated user, excluding completed/cancelled
/// ones (`completedAt` / `cancelledAt` null), newest-updated first. Field set is
/// exactly what `map_issue` reads — request only what the normalized item needs.
const ASSIGNED_ISSUES_QUERY: &str = r#"query Assigned($first: Int!) {
  viewer {
    assignedIssues(
      first: $first
      orderBy: updatedAt
      filter: { completedAt: { null: true }, canceledAt: { null: true } }
    ) {
      nodes {
        id
        identifier
        title
        url
        updatedAt
        priorityLabel
        state { name }
        project { name }
        assignee { displayName }
      }
    }
  }
}"#;

/// Fetch the brief's tasks from Linear and normalize them.
pub async fn fetch(
    _conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<FetchOutcome, FetchError> {
    if sel.kind != "tasks" {
        return Err(format!(
            "Linear supports kind: tasks (got \"{}\"). Notifications come in a later phase.",
            sel.kind
        )
        .into());
    }
    let first = sel.limit.unwrap_or(20).clamp(1, 100);
    let body = serde_json::json!({
        "query": ASSIGNED_ISSUES_QUERY,
        "variables": { "first": first },
    });
    let (json, meta) = post(token, &body).await?;
    let nodes = json
        .pointer("/data/viewer/assignedIssues/nodes")
        .and_then(|v| v.as_array())
        .ok_or("unexpected Linear response shape (no assignedIssues.nodes)")?;
    Ok(FetchOutcome::fresh(nodes.iter().map(map_issue).collect(), 1).with_meta(&meta))
}

/// Cheap auth check: ask for the viewer's id.
pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let body = serde_json::json!({ "query": "query { viewer { id } }" });
    post(token, &body).await.map(|_| ()).map_err(|e| e.message)
}

/// Map one Linear issue node into a normalized `IntegrationItem`. Pulled out so
/// it can be unit-tested against a saved JSON fixture (no network).
fn map_issue(node: &serde_json::Value) -> IntegrationItem {
    let s = |k: &str| node.get(k).and_then(|v| v.as_str()).map(String::from);
    let pointer = |p: &str| node.pointer(p).and_then(|v| v.as_str()).map(String::from);

    let title = s("title").unwrap_or_default();
    // Prefix the human identifier ("ENG-123 · …") so the list reads like Linear.
    let display = match s("identifier") {
        Some(id) if !id.is_empty() => format!("{id} · {title}"),
        _ => title,
    };

    let mut meta = BTreeMap::new();
    if let Some(project) = pointer("/project/name") {
        meta.insert("project".to_string(), project);
    }
    if let Some(priority) = s("priorityLabel") {
        if !priority.is_empty() && priority != "No priority" {
            meta.insert("priority".to_string(), priority);
        }
    }

    IntegrationItem {
        id: s("id").unwrap_or_default(),
        title: display,
        url: s("url").unwrap_or_default(),
        status: pointer("/state/name"),
        assignee: pointer("/assignee/displayName"),
        updated_at: s("updatedAt"),
        kind: "task".to_string(),
        meta,
    }
}

/// POST a GraphQL body and return the parsed JSON, surfacing auth failures and
/// GraphQL-level `errors` as clear messages. Builds its own short-timeout client
/// so a dead network fails fast instead of hanging the UI.
async fn post(
    token: &str,
    body: &serde_json::Value,
) -> Result<(serde_json::Value, super::ResponseMeta), FetchError> {
    let resp = super::http_client()?
        .post(ENDPOINT)
        .header(reqwest::header::AUTHORIZATION, token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .await
        .map_err(FetchError::network)?;

    // Linear answers an exhausted limit with a 400 whose body carries the
    // `RATELIMITED` code; promote it to a typed rate limit.
    let read = match super::read_response(resp, "Linear").await {
        Err(e) if e.message.contains("RATELIMITED") => {
            return Err(FetchError::rate_limited("Linear rate limit hit — WAID will wait before trying again.", None))
        }
        other => other?,
    };
    let meta = read.meta.clone();
    let json = read.json("Linear")?;

    // GraphQL reports errors in-band with a 200; surface the first one.
    if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
        if let Some(first) = errors.first() {
            if first.pointer("/extensions/code").and_then(|c| c.as_str()) == Some("RATELIMITED") {
                return Err(FetchError::rate_limited(
                    "Linear rate limit hit — WAID will wait before trying again.",
                    meta.retry_after_ms,
                ));
            }
            let msg = first
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(format!("Linear API error: {msg}").into());
        }
    }
    Ok((json, meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_issue_node_to_normalized_item() {
        let node: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "abc-123",
                "identifier": "ENG-42",
                "title": "Fix the thing",
                "url": "https://linear.app/acme/issue/ENG-42",
                "updatedAt": "2026-06-01T10:00:00.000Z",
                "priorityLabel": "High",
                "state": { "name": "In Progress" },
                "project": { "name": "Platform" },
                "assignee": { "displayName": "Jelani" }
            }"#,
        )
        .unwrap();

        let item = map_issue(&node);
        assert_eq!(item.id, "abc-123");
        assert_eq!(item.title, "ENG-42 · Fix the thing");
        assert_eq!(item.url, "https://linear.app/acme/issue/ENG-42");
        assert_eq!(item.status.as_deref(), Some("In Progress"));
        assert_eq!(item.assignee.as_deref(), Some("Jelani"));
        assert_eq!(item.updated_at.as_deref(), Some("2026-06-01T10:00:00.000Z"));
        assert_eq!(item.kind, "task");
        assert_eq!(item.meta.get("project").map(String::as_str), Some("Platform"));
        assert_eq!(item.meta.get("priority").map(String::as_str), Some("High"));
    }

    #[test]
    fn map_issue_omits_empty_priority_and_missing_fields() {
        let node: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "x",
                "title": "No frills",
                "url": "https://linear.app/x",
                "priorityLabel": "No priority",
                "state": { "name": "Todo" }
            }"#,
        )
        .unwrap();

        let item = map_issue(&node);
        // No identifier → title used as-is.
        assert_eq!(item.title, "No frills");
        assert_eq!(item.status.as_deref(), Some("Todo"));
        assert_eq!(item.assignee, None);
        assert_eq!(item.updated_at, None);
        // "No priority" and the absent project are both omitted from meta.
        assert!(item.meta.is_empty());
    }
}
