//! Asana provider — REST with a personal access token.
//!
//! Auth is `Bearer <PAT>`. v1 implements `kind: tasks` as "incomplete tasks
//! assigned to me" in a workspace. The workspace gid comes from the connection's
//! `account` when set; otherwise the user's first workspace is used. The brief's
//! `query` is reserved for a later phase.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, FetchCtx, FetchError, FetchOutcome, IntegrationItem};

const BASE: &str = "https://app.asana.com/api/1.0";

/// `opt_fields` requested for each task — exactly what `map_task` reads.
const TASK_FIELDS: &str =
    "name,permalink_url,completed,assignee.name,due_on,modified_at,memberships.project.name";

pub async fn fetch(
    conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
    ctx: &FetchCtx<'_>,
) -> Result<FetchOutcome, FetchError> {
    if sel.kind != "tasks" {
        return Err(format!("Asana supports kind: tasks (got \"{}\").", sel.kind).into());
    }
    let client = super::http_client()?;
    // Pinned workspace, else the cached first-workspace lookup (aux), else ask.
    let pinned = conn.account.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let cached = ctx.aux.and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let (workspace, looked_up) = match (pinned, cached) {
        (Some(w), _) | (None, Some(w)) => (w.to_string(), false),
        (None, None) => (first_workspace(&client, token).await?, true),
    };
    let max = sel.limit.unwrap_or(20).clamp(1, 100);

    // `completed_since=now` returns only incomplete tasks.
    let url = format!(
        "{BASE}/tasks?assignee=me&workspace={workspace}&completed_since=now&limit={max}&opt_fields={TASK_FIELDS}"
    );
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(FetchError::network)?;
    let read = super::read_response(resp, "Asana").await?;
    let meta = read.meta.clone();
    let json = read.json("Asana")?;
    let data = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or("unexpected Asana response (no data)")?;
    let items = data.iter().map(map_task).collect();
    Ok(FetchOutcome::fresh(items, 1 + looked_up as u32)
        .with_meta(&meta)
        .with_aux(looked_up.then(|| serde_json::Value::String(workspace))))
}

pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let resp = super::http_client()?
        .get(format!("{BASE}/users/me"))
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    super::read_json(resp, "Asana").await.map(|_| ())
}

/// First workspace gid for the token's user (used when the connection doesn't
/// pin one).
async fn first_workspace(client: &reqwest::Client, token: &str) -> Result<String, FetchError> {
    let resp = client
        .get(format!("{BASE}/workspaces"))
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(FetchError::network)?;
    let json = super::read_response(resp, "Asana").await?.json("Asana")?;
    json.pointer("/data/0/gid")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "Asana account has no workspaces.".into())
}

/// Map one Asana task into a normalized item. Pure — fixture-tested.
fn map_task(task: &serde_json::Value) -> IntegrationItem {
    let s = |k: &str| task.get(k).and_then(|v| v.as_str()).map(String::from);

    let mut meta = BTreeMap::new();
    if let Some(p) = task
        .pointer("/memberships/0/project/name")
        .and_then(|v| v.as_str())
    {
        meta.insert("project".to_string(), p.to_string());
    }
    if let Some(d) = task.get("due_on").and_then(|v| v.as_str()) {
        meta.insert("due".to_string(), d.to_string());
    }

    IntegrationItem {
        id: s("gid").unwrap_or_default(),
        title: s("name").unwrap_or_default(),
        url: s("permalink_url").unwrap_or_default(),
        // Asana's "My Tasks" has no workflow status in this view; leave it off.
        status: None,
        assignee: task.pointer("/assignee/name").and_then(|v| v.as_str()).map(String::from),
        updated_at: s("modified_at"),
        kind: "task".to_string(),
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_asana_task() {
        let task: serde_json::Value = serde_json::from_str(
            r#"{
                "gid": "12345",
                "name": "Write the spec",
                "permalink_url": "https://app.asana.com/0/12345/12345",
                "completed": false,
                "assignee": { "name": "Jelani John" },
                "due_on": "2026-06-10",
                "modified_at": "2026-06-01T10:00:00.000Z",
                "memberships": [ { "project": { "name": "Roadmap" } } ]
            }"#,
        )
        .unwrap();

        let item = map_task(&task);
        assert_eq!(item.id, "12345");
        assert_eq!(item.title, "Write the spec");
        assert_eq!(item.url, "https://app.asana.com/0/12345/12345");
        assert_eq!(item.status, None);
        assert_eq!(item.assignee.as_deref(), Some("Jelani John"));
        assert_eq!(item.updated_at.as_deref(), Some("2026-06-01T10:00:00.000Z"));
        assert_eq!(item.meta.get("project").map(String::as_str), Some("Roadmap"));
        assert_eq!(item.meta.get("due").map(String::as_str), Some("2026-06-10"));
    }

    #[test]
    fn map_asana_task_tolerates_missing_fields() {
        let task: serde_json::Value =
            serde_json::from_str(r#"{ "gid": "9", "name": "bare" }"#).unwrap();
        let item = map_task(&task);
        assert_eq!(item.title, "bare");
        assert_eq!(item.assignee, None);
        assert!(item.meta.is_empty());
    }
}
