//! Notion provider — REST with an internal integration token.
//!
//! Auth is `Bearer <token>` plus a required `Notion-Version` header. The token
//! only sees databases/pages the user has explicitly **shared** with the
//! integration (via each page's *Connections* menu) — a fetch against an
//! unshared resource 401/403s and degrades to no-signal upstream.
//!
//! Two roles, one connection (see the integration spec):
//! - **`fetch`** queries a database's rows as `IntegrationItem`s for the panel.
//! - **`fetch_page_text`** pulls a page's block text as synthesis evidence.
//!
//! Pure / network-only like the sibling providers: the token is loaded by the
//! command layer and passed in; this module never touches disk or the keyring.
//!
//! 2025-09-03 data-source model: a database now contains one or more *data
//! sources*, and row queries hit `/data_sources/{ds_id}/query` rather than
//! `/databases/{id}/query`. v1 resolves `data_sources[0]` and notes the limit.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, IntegrationItem};

const BASE: &str = "https://api.notion.com/v1";
const VERSION: &str = "2025-09-03";

/// Apply the headers every Notion request needs: bearer auth, the required API
/// version, and a JSON accept.
fn with_headers(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    rb.bearer_auth(token)
        .header("Notion-Version", VERSION)
        .header(reqwest::header::ACCEPT, "application/json")
}

pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let resp = with_headers(super::http_client()?.get(format!("{BASE}/users/me")), token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    super::read_json(resp, "Notion").await.map(|_| ())
}

pub async fn fetch(
    _conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    if sel.kind != "tasks" {
        return Err(format!("Notion supports kind: tasks (got \"{}\").", sel.kind));
    }
    let db_id = extract_id(sel.query.as_deref().unwrap_or_default())
        .ok_or("Notion selector needs a database id or URL in `query`.")?;
    let client = super::http_client()?;
    let ds_id = resolve_data_source(&client, token, &db_id).await?;
    let max = sel.limit.unwrap_or(25).clamp(1, 100);

    let body = serde_json::json!({
        "page_size": max,
        "sorts": [{ "timestamp": "last_edited_time", "direction": "descending" }],
    });
    let resp = with_headers(
        client.post(format!("{BASE}/data_sources/{ds_id}/query")),
        token,
    )
    .json(&body)
    .send()
    .await
    .map_err(|e| format!("request failed: {e}"))?;
    let json = super::read_json(resp, "Notion").await?;
    let results = json
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or("unexpected Notion response (no results)")?;
    Ok(results.iter().map(map_page).collect())
}

/// Resolve a database id to its first data source's id (2025-09-03 model). Falls
/// back to the database id itself when the array is absent — older workspaces, or
/// hosts still answering the legacy shape.
async fn resolve_data_source(
    client: &reqwest::Client,
    token: &str,
    db_id: &str,
) -> Result<String, String> {
    let resp = with_headers(client.get(format!("{BASE}/databases/{db_id}")), token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let json = super::read_json(resp, "Notion").await?;
    Ok(json
        .pointer("/data_sources/0/id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| db_id.to_string()))
}

/// Pull a Notion object id (32 hex) from a pasted id or URL. Strips dashes, drops
/// any `?v=<view-id>` query, and takes the trailing 32 hex of the last path
/// segment — so a page slug's trailing letters can't shift the id. Pure. Also
/// reused by the command layer to route `notion.so` evidence links (Part B).
pub fn extract_id(s: &str) -> Option<String> {
    let path = s.split('?').next().unwrap_or(s);
    let seg = path.rsplit('/').find(|p| !p.is_empty()).unwrap_or(path);
    let hex: String = seg.chars().filter(|c| *c != '-').collect();
    let trailing: Vec<char> = hex
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if trailing.len() < 32 {
        return None;
    }
    // `trailing` is reversed; the id is its first 32 chars un-reversed.
    Some(trailing.into_iter().take(32).rev().collect())
}

/// Map one Notion database row (a page) into a normalized item. Each property
/// carries its own `type`, so no schema fetch is needed. Pure — fixture-tested.
/// Never fabricates a status/assignee that isn't clearly present (no-signal rule).
fn map_page(page: &serde_json::Value) -> IntegrationItem {
    let props = page.get("properties").and_then(|v| v.as_object());

    let title = props
        .and_then(|m| {
            m.values()
                .find(|p| p.get("type").and_then(|t| t.as_str()) == Some("title"))
        })
        .and_then(|p| p.get("title"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("plain_text").and_then(|s| s.as_str()))
                .collect::<String>()
        })
        .unwrap_or_default();

    IntegrationItem {
        id: page
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        title,
        url: page
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        status: props.and_then(find_status),
        assignee: props.and_then(find_assignee),
        updated_at: page
            .get("last_edited_time")
            .and_then(|v| v.as_str())
            .map(String::from),
        kind: "task".to_string(),
        meta: props.map(build_meta).unwrap_or_default(),
    }
}

/// Status = first `status` property, else a `select` whose key is (case-
/// insensitive) status/stage/state. Returns `None` when nothing clearly matches.
fn find_status(props: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    for p in props.values() {
        if p.get("type").and_then(|t| t.as_str()) == Some("status") {
            if let Some(name) = p.pointer("/status/name").and_then(|v| v.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    for (k, p) in props {
        let key = k.to_lowercase();
        if matches!(key.as_str(), "status" | "stage" | "state")
            && p.get("type").and_then(|t| t.as_str()) == Some("select")
        {
            if let Some(name) = p.pointer("/select/name").and_then(|v| v.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Assignee = first `people` property's first person name.
fn find_assignee(props: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    for p in props.values() {
        if p.get("type").and_then(|t| t.as_str()) == Some("people") {
            if let Some(name) = p.pointer("/people/0/name").and_then(|v| v.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Light extras only: a `due`/`due date` date and a `priority` select/status.
/// Everything else is dropped (no-signal).
fn build_meta(props: &serde_json::Map<String, serde_json::Value>) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();
    for (k, p) in props {
        let key = k.to_lowercase();
        let ty = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if matches!(key.as_str(), "due" | "due date") && ty == "date" {
            if let Some(d) = p.pointer("/date/start").and_then(|v| v.as_str()) {
                meta.insert("due".to_string(), d.to_string());
            }
        }
        if key == "priority" && matches!(ty, "select" | "status") {
            if let Some(v) = p.pointer(&format!("/{ty}/name")).and_then(|v| v.as_str()) {
                meta.insert("priority".to_string(), v.to_string());
            }
        }
    }
    meta
}

// --- Part B: page text as synthesis evidence -------------------------------

/// Block types that carry readable `rich_text` we extract. Anything else (images,
/// dividers, child databases, …) contributes no text.
const TEXT_BLOCKS: &[&str] = &[
    "paragraph",
    "heading_1",
    "heading_2",
    "heading_3",
    "bulleted_list_item",
    "numbered_list_item",
    "to_do",
    "quote",
    "callout",
    "code",
    "toggle",
];

/// Fetch a Notion page's readable text. Paginates the page's children, recurses
/// **one level** into blocks with children (bounded), and reduces to plain lines.
/// The caller truncates to its budget. A non-2xx (page not shared, bad token)
/// returns `Err`, so the caller skips it as no-signal. Network-only.
pub async fn fetch_page_text(token: &str, page_id: &str) -> Result<String, String> {
    let client = super::http_client()?;
    let top = fetch_children(&client, token, page_id).await?;
    // Flatten parent-then-its-children (one level) so `reduce_blocks` can do the
    // pure text reduction the fixture test covers.
    let mut flat: Vec<serde_json::Value> = Vec::new();
    for block in top {
        let has_children = block
            .get("has_children")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let id = block.get("id").and_then(|v| v.as_str()).map(String::from);
        flat.push(block);
        if has_children {
            if let Some(id) = id {
                if let Ok(kids) = fetch_children(&client, token, &id).await {
                    flat.extend(kids);
                }
            }
        }
    }
    Ok(reduce_blocks(&flat))
}

/// Page through `GET /blocks/{id}/children` via `next_cursor` / `has_more`.
async fn fetch_children(
    client: &reqwest::Client,
    token: &str,
    block_id: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!("{BASE}/blocks/{block_id}/children?page_size=100");
        if let Some(c) = &cursor {
            url.push_str(&format!("&start_cursor={c}"));
        }
        let resp = with_headers(client.get(&url), token)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let json = super::read_json(resp, "Notion").await?;
        if let Some(arr) = json.get("results").and_then(|v| v.as_array()) {
            out.extend(arr.iter().cloned());
        }
        if json.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false) {
            cursor = json.get("next_cursor").and_then(|v| v.as_str()).map(String::from);
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }
    Ok(out)
}

/// One block → its line of text, or `None` for an unsupported block type. A
/// supported block with empty `rich_text` yields an empty line (paragraph break),
/// which `collapse_blank_runs` then dedupes.
fn block_to_text(block: &serde_json::Value) -> Option<String> {
    let ty = block.get("type").and_then(|v| v.as_str())?;
    if !TEXT_BLOCKS.contains(&ty) {
        return None;
    }
    let text = block
        .pointer(&format!("/{ty}/rich_text"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("plain_text").and_then(|s| s.as_str()))
                .collect::<String>()
        })
        .unwrap_or_default();
    Some(text)
}

/// Reduce a flat array of blocks to text: one line per text block, blank runs
/// collapsed, trimmed. Pure — fixture-tested.
fn reduce_blocks(blocks: &[serde_json::Value]) -> String {
    let lines: Vec<String> = blocks.iter().filter_map(block_to_text).collect();
    let mut out: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for l in &lines {
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(l.as_str());
        prev_blank = blank;
    }
    out.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_notion_page() {
        let page: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d",
                "url": "https://www.notion.so/Write-the-spec-2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d",
                "last_edited_time": "2026-06-01T10:00:00.000Z",
                "properties": {
                    "Name": { "type": "title", "title": [ { "plain_text": "Write the spec" } ] },
                    "Status": { "type": "status", "status": { "name": "In Progress" } },
                    "Owner": { "type": "people", "people": [ { "name": "Jelani John" } ] },
                    "Due": { "type": "date", "date": { "start": "2026-06-10" } },
                    "Priority": { "type": "select", "select": { "name": "High" } }
                }
            }"#,
        )
        .unwrap();

        let item = map_page(&page);
        assert_eq!(item.id, "2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d");
        assert_eq!(item.title, "Write the spec");
        assert_eq!(
            item.url,
            "https://www.notion.so/Write-the-spec-2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d"
        );
        assert_eq!(item.status.as_deref(), Some("In Progress"));
        assert_eq!(item.assignee.as_deref(), Some("Jelani John"));
        assert_eq!(item.updated_at.as_deref(), Some("2026-06-01T10:00:00.000Z"));
        assert_eq!(item.meta.get("due").map(String::as_str), Some("2026-06-10"));
        assert_eq!(item.meta.get("priority").map(String::as_str), Some("High"));
        assert_eq!(item.kind, "task");
    }

    #[test]
    fn map_notion_page_tolerates_missing_fields() {
        let page: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "9",
                "url": "https://www.notion.so/9",
                "last_edited_time": "2026-06-01T10:00:00.000Z"
            }"#,
        )
        .unwrap();
        let item = map_page(&page);
        assert_eq!(item.id, "9");
        assert_eq!(item.title, "");
        assert_eq!(item.status, None);
        assert_eq!(item.assignee, None);
        assert!(item.meta.is_empty());
    }

    #[test]
    fn find_status_falls_back_to_named_select() {
        let props: serde_json::Value = serde_json::from_str(
            r#"{
                "Stage": { "type": "select", "select": { "name": "Backlog" } }
            }"#,
        )
        .unwrap();
        assert_eq!(
            find_status(props.as_object().unwrap()).as_deref(),
            Some("Backlog")
        );
    }

    const ID: &str = "2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d";

    #[test]
    fn extract_id_from_bare_id() {
        assert_eq!(extract_id(ID).as_deref(), Some(ID));
    }

    #[test]
    fn extract_id_from_dashed_id() {
        let dashed = "2f1baf9c-8d7e-4a3b-9c0d-1e2f3a4b5c6d";
        assert_eq!(extract_id(dashed).as_deref(), Some(ID));
    }

    #[test]
    fn extract_id_from_page_url() {
        let url = "https://www.notion.so/My-Page-Title-2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d";
        assert_eq!(extract_id(url).as_deref(), Some(ID));
    }

    #[test]
    fn extract_id_from_database_url_with_view() {
        let url = "https://www.notion.so/myws/2f1baf9c8d7e4a3b9c0d1e2f3a4b5c6d?v=8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e";
        assert_eq!(extract_id(url).as_deref(), Some(ID));
    }

    #[test]
    fn extract_id_rejects_non_id() {
        assert_eq!(extract_id("https://www.notion.so/just-a-slug"), None);
        assert_eq!(extract_id(""), None);
    }

    #[test]
    fn reduces_blocks_to_text() {
        let blocks: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
                { "type": "heading_1", "heading_1": { "rich_text": [ { "plain_text": "Overview" } ] } },
                { "type": "paragraph", "paragraph": { "rich_text": [ { "plain_text": "Ship the " }, { "plain_text": "integration." } ] } },
                { "type": "paragraph", "paragraph": { "rich_text": [] } },
                { "type": "paragraph", "paragraph": { "rich_text": [] } },
                { "type": "bulleted_list_item", "bulleted_list_item": { "rich_text": [ { "plain_text": "First point" } ] } },
                { "type": "image", "image": {} },
                { "type": "to_do", "to_do": { "rich_text": [ { "plain_text": "A task" } ] } }
            ]"#,
        )
        .unwrap();

        let text = reduce_blocks(&blocks);
        assert_eq!(
            text,
            "Overview\nShip the integration.\n\nFirst point\nA task"
        );
    }
}
