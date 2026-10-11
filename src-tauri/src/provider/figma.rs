//! Figma provider — recent file comments as panel items, optionally filtered to
//! ones that @-mention the authenticated user.
//!
//! Token-paste (Personal Access Token, `X-Figma-Token` header), so it rides the
//! `bconn:` keyring flow unchanged — like Slack, this module is pure/network-only
//! and is handed a ready token. The per-brief `query` is REQUIRED and carries a
//! Figma file URL/key (the scope — the API has no cross-file comment search) plus
//! an optional `mentions:me` token.
//!
//! The honest caveat: the REST API has no structured mention list on a GET'd
//! comment, so `mentions:me` is a best-effort substring match on the flattened
//! `message` against the authenticated user's handle (from `/v1/me`). The default
//! (no `mentions:me`) surfaces recent **unresolved** comments — a deterministic,
//! always-correct signal — and `mentions:me` narrows on top.

use std::collections::BTreeMap;

use super::{BriefIntegration, Connection, FetchCtx, FetchError, FetchOutcome, IntegrationItem};

const BASE: &str = "https://api.figma.com/v1";

/// Apply the headers every Figma request needs. Figma authenticates with a
/// **Personal Access Token in `X-Figma-Token`** — *not* bearer auth — so do not
/// reuse a bearer helper here.
fn with_token(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    rb.header("X-Figma-Token", token)
        .header(reqwest::header::ACCEPT, "application/json")
}

/// Cheap credential check: hit `GET /v1/me`. `read_json`'s 401/403 mapping
/// already gives a friendly "rejected the credentials" message; a 403 here also
/// commonly means the token is missing `file_comments:read`, so add a hint.
pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let resp = with_token(super::http_client()?.get(format!("{BASE}/me")), token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("Figma rejected the token — it may be expired or missing the \
file_comments:read scope. Regenerate it in Settings → Account → Personal access tokens."
            .to_string());
    }
    super::read_json(resp, "Figma").await.map(|_| ())
}

pub async fn fetch(
    _conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
    ctx: &FetchCtx<'_>,
) -> Result<FetchOutcome, FetchError> {
    match sel.kind.as_str() {
        "comments" => fetch_comments(sel, token, ctx.aux).await,
        other => Err(format!("Figma supports kind: comments (got \"{other}\").").into()),
    }
}

/// Fetch one file's comments, optionally narrowed to @-mentions of the
/// authenticated user. The per-brief `query` carries the file URL/key (required)
/// plus an optional `mentions:me` token. `aux` is the cached `/v1/me` handle.
async fn fetch_comments(
    sel: &BriefIntegration,
    token: &str,
    aux: Option<&serde_json::Value>,
) -> Result<FetchOutcome, FetchError> {
    let raw = sel
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or("Figma selector needs a file URL or key (e.g. `figma.com/design/AbC123/…`).")?;
    let (file_key, mentions_only) = parse_query(raw)?;

    let resp = with_token(
        super::http_client()?.get(format!("{BASE}/files/{file_key}/comments")),
        token,
    )
    // `as_md=true` degrades rich-text comments to plain markdown cleanly.
    .query(&[("as_md", "true")])
    .send()
    .await
    .map_err(FetchError::network)?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Figma file not found, or the token's account can't see it — \
check the URL and that you have access."
            .into());
    }
    let read = super::read_response(resp, "Figma").await?;
    let meta = read.meta.clone();
    let json = read.json("Figma")?;

    let comments: Vec<&serde_json::Value> = json
        .get("comments")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default();

    // For `mentions:me`, learn the handle. On any `/v1/me` failure, degrade to
    // returning all comments rather than erroring (best-effort, like Slack's
    // `users.list`).
    let cached = aux.and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
    let handle_fetched = mentions_only && cached.is_none();
    let handle = match (mentions_only, cached) {
        (false, _) => None,
        (true, Some(h)) => Some(h),
        (true, None) => fetch_my_handle(token).await,
    };

    let mut kept: Vec<&serde_json::Value> = comments
        .into_iter()
        .filter(|c| {
            let message = c.get("message").and_then(|v| v.as_str()).unwrap_or("");
            match &handle {
                // mentions:me with a known handle → keep matches regardless of
                // resolved state (you still want a resolved thread you were
                // tagged in).
                Some(h) => mentions_me(message, h),
                // Default (or mentions:me that couldn't resolve a handle) → keep
                // unresolved comments only: a deterministic "what's open" signal.
                None => c.get("resolved_at").map(|v| v.is_null()).unwrap_or(true),
            }
        })
        .collect();

    // Recency first, then cap.
    kept.sort_by(|a, b| {
        let at = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let bt = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        bt.cmp(at)
    });
    let max = sel.limit.unwrap_or(15).clamp(1, 50) as usize;
    let items = kept.iter().take(max).map(|c| map_comment(c)).collect();
    let fresh_aux = handle.filter(|_| handle_fetched).map(serde_json::Value::String);
    Ok(FetchOutcome::fresh(items, 1 + handle_fetched as u32)
        .with_meta(&meta)
        .with_aux(fresh_aux))
}

/// Best-effort `GET /v1/me` → the authenticated user's handle for the mention
/// filter. Returns `None` on any failure so the caller degrades to all comments.
async fn fetch_my_handle(token: &str) -> Option<String> {
    let client = super::http_client().ok()?;
    let resp = with_token(client.get(format!("{BASE}/me")), token)
        .send()
        .await
        .ok()?;
    let json = super::read_json(resp, "Figma").await.ok()?;
    json.get("handle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Split the per-brief query into `(file_key, mentions_only)`: strip a
/// `mentions:me` / `@me` token (case-insensitive, anywhere), parse the rest as a
/// file key/URL. Pure — unit-tested.
fn parse_query(q: &str) -> Result<(String, bool), String> {
    let mut mentions_only = false;
    let remainder: Vec<&str> = q
        .split_whitespace()
        .filter(|tok| {
            if tok.eq_ignore_ascii_case("mentions:me") || tok.eq_ignore_ascii_case("@me") {
                mentions_only = true;
                false
            } else {
                true
            }
        })
        .collect();
    let rest = remainder.join(" ");
    parse_file_key(&rest)
        .map(|key| (key, mentions_only))
        .ok_or_else(|| {
            "Could not read a Figma file from the query — paste a file URL \
(figma.com/design/…) or the bare key."
                .to_string()
        })
}

/// Pull a Figma file key out of a bare key or a file URL. Accepts the segment
/// after `/file/`, `/design/`, `/board/` (FigJam), or `/proto/`; tolerates a
/// trailing `/title`, `?query`, and `#fragment`. Pure — unit-tested.
fn parse_file_key(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // URL form: find the path segment after one of the known markers.
    for marker in ["/file/", "/design/", "/board/", "/proto/"] {
        if let Some(idx) = s.find(marker) {
            let after = &s[idx + marker.len()..];
            let key = first_segment(after);
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
    }
    // Bare key: no scheme/slash, plausible key characters only.
    if !s.contains("://") && !s.contains('/') && is_plausible_key(s) {
        return Some(s.to_string());
    }
    None
}

/// First path segment, stripping any `?query`/`#fragment`.
fn first_segment(s: &str) -> &str {
    let s = s.split(['?', '#']).next().unwrap_or(s);
    s.split('/').next().unwrap_or(s)
}

/// A bare key is alphanumeric (Figma keys are base62-ish); reject obvious junk.
fn is_plausible_key(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Best-effort "does this comment @-mention `handle`?" There is no structured
/// mention list on a GET'd comment, so this is a substring test: the lower-cased
/// `message` contains `@{handle}`, else contains `{handle}` as a
/// whitespace/punctuation-bounded token. Pure — unit-tested.
fn mentions_me(message: &str, handle: &str) -> bool {
    let msg = message.to_lowercase();
    let needle = handle.to_lowercase();
    if needle.is_empty() {
        return false;
    }
    // Bounded match: the handle surrounded by non-alphanumerics. `@` counts as a
    // boundary, so this covers both `@jelani` and a bare `jelani` token while
    // rejecting `@jelanixyz` (the `x` is alphanumeric).
    let bytes = msg.as_bytes();
    let mut from = 0;
    while let Some(rel) = msg[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0
            || !bytes
                .get(start - 1)
                .map(|b| b.is_ascii_alphanumeric())
                .unwrap_or(false);
        let after_ok = !bytes
            .get(end)
            .map(|b| b.is_ascii_alphanumeric())
            .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        from = start + needle.len();
    }
    false
}

/// Map one comment object into a normalized item. Pure — fixture-tested. Never
/// fabricates fields that aren't present.
fn map_comment(c: &serde_json::Value) -> IntegrationItem {
    let id = c
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let message = c.get("message").and_then(|v| v.as_str()).unwrap_or_default();
    let title = {
        let first = message.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let first = first.trim();
        if first.is_empty() {
            "(no text)".to_string()
        } else {
            first.chars().take(100).collect::<String>()
        }
    };

    let file_key = c.get("file_key").and_then(|v| v.as_str()).unwrap_or_default();
    // No per-comment anchor exists in the API, so this lands in the file, not on
    // the exact comment pin.
    let url = format!("https://www.figma.com/design/{file_key}/");

    let resolved = c
        .get("resolved_at")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    let status = Some(if resolved { "Resolved" } else { "Open" }.to_string());

    let author = c
        .pointer("/user/handle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let updated_at = c
        .get("created_at")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let mut meta = BTreeMap::new();
    if let Some(a) = &author {
        meta.insert("author".to_string(), a.clone());
    }
    if c.get("parent_id").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty()) {
        meta.insert("reply".to_string(), "true".to_string());
    }
    if resolved {
        meta.insert("resolved".to_string(), "true".to_string());
    }
    if let Some(node) = c
        .pointer("/client_meta/node_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        meta.insert("node".to_string(), node.to_string());
    }
    let clean = message.trim();
    if !clean.is_empty() {
        meta.insert(
            "snippet".to_string(),
            clean.chars().take(200).collect::<String>(),
        );
    }

    IntegrationItem {
        id,
        title,
        url,
        status,
        // The comment author lives in the panel's "assignee" slot — same "who
        // said it" convention as Slack's sender / Gmail's From.
        assignee: author,
        // `created_at` is already RFC3339 — no conversion dance.
        updated_at,
        kind: "comments".to_string(),
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::from_str(
            r#"{
                "id": "comment-123",
                "file_key": "AbC123",
                "message": "Hey @jelani can you review the spacing here?\nSecond line",
                "user": { "id": "9", "handle": "dana", "img_url": "x" },
                "created_at": "2026-06-03T00:00:00Z",
                "order_id": "1",
                "client_meta": { "node_id": "4:5", "node_offset": { "x": 1, "y": 2 } }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn maps_comment() {
        let item = map_comment(&sample());
        assert_eq!(item.id, "comment-123");
        assert_eq!(item.title, "Hey @jelani can you review the spacing here?");
        assert_eq!(item.url, "https://www.figma.com/design/AbC123/");
        assert_eq!(item.status.as_deref(), Some("Open"));
        assert_eq!(item.assignee.as_deref(), Some("dana"));
        assert_eq!(item.updated_at.as_deref(), Some("2026-06-03T00:00:00Z"));
        assert_eq!(item.kind, "comments");
        assert_eq!(item.meta.get("author").map(String::as_str), Some("dana"));
        assert_eq!(item.meta.get("node").map(String::as_str), Some("4:5"));
        assert!(item.meta.get("reply").is_none());
        assert!(item.meta.get("resolved").is_none());
        assert_eq!(
            item.meta.get("snippet").map(String::as_str),
            Some("Hey @jelani can you review the spacing here?\nSecond line")
        );
        // created_at round-trips through chrono's RFC3339 parser (summarize uses it).
        assert!(chrono::DateTime::parse_from_rfc3339(item.updated_at.as_deref().unwrap()).is_ok());
    }

    #[test]
    fn tolerates_missing_fields() {
        let c: serde_json::Value = serde_json::from_str(r#"{ "id": "c1" }"#).unwrap();
        let item = map_comment(&c);
        assert_eq!(item.id, "c1");
        assert_eq!(item.title, "(no text)");
        assert_eq!(item.assignee, None);
        assert_eq!(item.updated_at, None);
        // No file_key → bare /design// URL, still well-formed.
        assert_eq!(item.url, "https://www.figma.com/design//");
        assert!(item.meta.is_empty());
    }

    #[test]
    fn resolved_and_reply_meta() {
        let c: serde_json::Value = serde_json::from_str(
            r#"{
                "id": "c2",
                "file_key": "K",
                "message": "ok done",
                "user": { "handle": "sam" },
                "parent_id": "comment-123",
                "resolved_at": "2026-06-04T00:00:00Z",
                "created_at": "2026-06-04T00:00:00Z"
            }"#,
        )
        .unwrap();
        let item = map_comment(&c);
        assert_eq!(item.status.as_deref(), Some("Resolved"));
        assert_eq!(item.meta.get("reply").map(String::as_str), Some("true"));
        assert_eq!(item.meta.get("resolved").map(String::as_str), Some("true"));
    }

    #[test]
    fn parses_file_key() {
        assert_eq!(parse_file_key("AbC123").as_deref(), Some("AbC123"));
        assert_eq!(
            parse_file_key("https://www.figma.com/file/AbC123/Solaris-UI").as_deref(),
            Some("AbC123")
        );
        assert_eq!(
            parse_file_key("https://www.figma.com/design/AbC123/Solaris-UI").as_deref(),
            Some("AbC123")
        );
        assert_eq!(
            parse_file_key("https://www.figma.com/board/Xy9/Brainstorm").as_deref(),
            Some("Xy9")
        );
        assert_eq!(
            parse_file_key("https://www.figma.com/proto/Pr0t0/Flow").as_deref(),
            Some("Pr0t0")
        );
        // Query string + fragment after the key are tolerated.
        assert_eq!(
            parse_file_key("https://www.figma.com/design/AbC123/UI?node-id=4-5#hi").as_deref(),
            Some("AbC123")
        );
        // Bare key tolerated; trailing-segment-less URL too.
        assert_eq!(parse_file_key("https://www.figma.com/design/Zz").as_deref(), Some("Zz"));
        // Junk → None.
        assert_eq!(parse_file_key("not a url or key !!"), None);
        assert_eq!(parse_file_key(""), None);
    }

    #[test]
    fn parses_query() {
        // mentions:me at the end.
        let (k, m) = parse_query("https://www.figma.com/file/AbC123/UI mentions:me").unwrap();
        assert_eq!(k, "AbC123");
        assert!(m);
        // @me at the front, case-insensitive.
        let (k, m) = parse_query("@ME AbC123").unwrap();
        assert_eq!(k, "AbC123");
        assert!(m);
        // No mention token → false.
        let (k, m) = parse_query("AbC123").unwrap();
        assert_eq!(k, "AbC123");
        assert!(!m);
        // Mention token but no file → error.
        assert!(parse_query("mentions:me").is_err());
    }

    #[test]
    fn mentions_me_matching() {
        assert!(mentions_me("hey @jelani look", "jelani"));
        // Case-insensitive.
        assert!(mentions_me("Hey @Jelani", "jelani"));
        // Bare-handle token (no @) still matches when bounded.
        assert!(mentions_me("jelani: please check", "jelani"));
        // Token boundary: @jelanixyz is not a mention of jelani.
        assert!(!mentions_me("cc @jelanixyz", "jelani"));
        // Substring inside another word doesn't match.
        assert!(!mentions_me("unrelatedjelaniword", "jelani"));
        // Empty handle never matches.
        assert!(!mentions_me("@jelani", ""));
    }
}
