//! Gmail provider — surfaces recent, brief-relevant emails as panel items.
//!
//! Unlike the token-paste providers, Gmail authenticates with Google OAuth (the
//! desktop loopback + PKCE flow), but that all lives in the command layer: like
//! every sibling here, this module is **pure / network-only** and is handed a
//! ready bearer access token. It never refreshes, never touches the keyring.
//!
//! The per-brief selector's `query` carries a **Gmail search string**
//! (`from:acme.com subject:"redesign" newer_than:14d`) — that *is* the
//! assignment, so an empty query errors (mirrors Notion's "needs a query").
//!
//! **Metadata only (v1):** messages are fetched with `format=metadata` plus the
//! snippet — never `format=full`, never bodies. Subjects + snippets are all that
//! ever leave Gmail, so they're safe to flow into the digest / morning briefing.
//! Read-only scope (`gmail.readonly`); no send, no modify.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, IntegrationItem};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1";

/// Apply the headers every Gmail request needs: bearer auth + a JSON accept.
fn with_headers(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    rb.bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
}

/// Cheap credential check: hit the profile endpoint (works with `gmail.readonly`,
/// so no extra `userinfo` scope is needed). `read_json`'s 401/403 mapping already
/// produces the right "credentials rejected" message.
pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let resp = with_headers(
        super::http_client()?.get(format!("{BASE}/users/me/profile")),
        token,
    )
    .send()
    .await
    .map_err(|e| format!("request failed: {e}"))?;
    super::read_json(resp, "Gmail").await.map(|_| ())
}

pub async fn fetch(
    _conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    match sel.kind.as_str() {
        "email" => fetch_emails(sel, token).await,
        other => Err(format!("Gmail supports kind: email (got \"{other}\").")),
    }
}

/// List message ids matching the search string, then fetch each one's metadata
/// (sequential — the cap is modest) and map it. Empty query errors, since the
/// per-brief query *is* the assignment (mirrors Notion).
async fn fetch_emails(sel: &BriefIntegration, token: &str) -> Result<Vec<IntegrationItem>, String> {
    let q = sel
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or("Gmail selector needs a search query (e.g. `from:acme.com newer_than:14d`).")?;
    let max = sel.limit.unwrap_or(15).clamp(1, 50);

    let client = super::http_client()?;
    let resp = with_headers(client.get(format!("{BASE}/users/me/messages")), token)
        .query(&[("q", q), ("maxResults", &max.to_string())])
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let list = super::read_json(resp, "Gmail").await?;
    let ids: Vec<String> = list
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        let resp = with_headers(
            client.get(format!("{BASE}/users/me/messages/{id}")),
            token,
        )
        // metadata only — no bodies leave Gmail (see module docs).
        .query(&[
            ("format", "metadata"),
            ("metadataHeaders", "From"),
            ("metadataHeaders", "Subject"),
            ("metadataHeaders", "Date"),
        ])
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
        let msg = super::read_json(resp, "Gmail").await?;
        items.push(map_message(&msg));
    }
    Ok(items)
}

/// Case-insensitive lookup of a header value in `payload.headers: [{name, value}]`.
fn header(msg: &serde_json::Value, name: &str) -> Option<String> {
    msg.pointer("/payload/headers")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|h| {
                h.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
        })
        .and_then(|h| h.get("value").and_then(|v| v.as_str()))
        .map(String::from)
}

/// Parse Gmail's `internalDate` (a **string of epoch milliseconds**) into RFC3339.
/// `summarize`'s "updated recently" math depends on a parseable RFC3339 here.
/// Pure — unit-tested.
fn internal_date_to_rfc3339(ms: &str) -> Option<String> {
    let millis: i64 = ms.trim().parse().ok()?;
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}

/// Map one Gmail message (metadata form) into a normalized item. Pure —
/// fixture-tested. Never fabricates fields that aren't present.
fn map_message(msg: &serde_json::Value) -> IntegrationItem {
    let id = msg
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let from = header(msg, "From");

    let unread = msg
        .get("labelIds")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|l| l.as_str() == Some("UNREAD")))
        .unwrap_or(false);

    let snippet = msg
        .get("snippet")
        .and_then(|v| v.as_str())
        .map(|s| {
            let s = s.trim();
            // Keep it short — it's display + digest context only.
            s.chars().take(200).collect::<String>()
        })
        .filter(|s| !s.is_empty());

    let mut meta = BTreeMap::new();
    if let Some(f) = &from {
        meta.insert("from".to_string(), f.clone());
    }
    if let Some(s) = snippet {
        meta.insert("snippet".to_string(), s);
    }

    IntegrationItem {
        id: id.clone(),
        title: header(msg, "Subject")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "(no subject)".to_string()),
        url: format!("https://mail.google.com/mail/u/0/#all/{id}"),
        // The sender lives in the panel's "assignee" slot — the most natural
        // place to show "who this email is from."
        assignee: from,
        status: Some(if unread { "unread" } else { "read" }.to_string()),
        updated_at: msg
            .get("internalDate")
            .and_then(|v| v.as_str())
            .and_then(internal_date_to_rfc3339),
        kind: "email".to_string(),
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::from_str(
            r#"{
                "id": "18f0a1b2c3d4e5f6",
                "labelIds": ["INBOX", "UNREAD", "IMPORTANT"],
                "internalDate": "1717372800000",
                "snippet": "  Hey — wanted to check in on the redesign timeline before Friday.  ",
                "payload": {
                    "headers": [
                        { "name": "Delivered-To", "value": "me@acme.com" },
                        { "name": "From", "value": "Dana Lee <dana@acme.com>" },
                        { "name": "Subject", "value": "Acme redesign — timeline" },
                        { "name": "Date", "value": "Mon, 03 Jun 2024 00:00:00 +0000" }
                    ]
                }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn maps_email() {
        let item = map_message(&sample());
        assert_eq!(item.id, "18f0a1b2c3d4e5f6");
        assert_eq!(item.title, "Acme redesign — timeline");
        assert_eq!(
            item.url,
            "https://mail.google.com/mail/u/0/#all/18f0a1b2c3d4e5f6"
        );
        assert_eq!(item.assignee.as_deref(), Some("Dana Lee <dana@acme.com>"));
        assert_eq!(item.kind, "email");
        assert_eq!(item.meta.get("from").map(String::as_str), Some("Dana Lee <dana@acme.com>"));
        assert_eq!(
            item.meta.get("snippet").map(String::as_str),
            Some("Hey — wanted to check in on the redesign timeline before Friday.")
        );
    }

    #[test]
    fn map_email_tolerates_missing_subject_and_headers() {
        let msg: serde_json::Value = serde_json::from_str(
            r#"{ "id": "abc", "internalDate": "1717372800000" }"#,
        )
        .unwrap();
        let item = map_message(&msg);
        assert_eq!(item.id, "abc");
        assert_eq!(item.title, "(no subject)");
        assert_eq!(item.assignee, None);
        // No UNREAD label present → read.
        assert_eq!(item.status.as_deref(), Some("read"));
        assert!(item.meta.get("from").is_none());
    }

    #[test]
    fn status_reflects_unread_label() {
        let unread = map_message(&sample());
        assert_eq!(unread.status.as_deref(), Some("unread"));

        let mut read = sample();
        read["labelIds"] = serde_json::json!(["INBOX", "IMPORTANT"]);
        assert_eq!(map_message(&read).status.as_deref(), Some("read"));
    }

    #[test]
    fn internal_date_parses() {
        // 1717372800000 ms = 2024-06-03T00:00:00Z.
        assert_eq!(
            internal_date_to_rfc3339("1717372800000").as_deref(),
            Some("2024-06-03T00:00:00+00:00")
        );
        assert_eq!(internal_date_to_rfc3339("not-a-number"), None);
        // And it round-trips through chrono's RFC3339 parser (what summarize uses).
        let rfc = internal_date_to_rfc3339("1717372800000").unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&rfc).is_ok());
    }
}
