//! Slack provider — surfaces recent messages matching a search as panel items.
//!
//! A **token-paste** provider, not OAuth: the user creates their own Slack app,
//! adds `search:read` under **User Token Scopes**, installs it, and pastes the
//! resulting user token (`xoxp-…`). That rides the existing `bconn:` keyring flow
//! unchanged — like every sibling here, this module is **pure / network-only**
//! and is handed a ready bearer token; it never touches the keyring or disk.
//!
//! The per-brief selector's `query` carries a **Slack search string**
//! (`in:#waid from:@dana after:2026-06-01`) — that *is* the assignment, so an
//! empty query errors (mirrors Gmail/Notion).
//!
//! **Read-only:** the only call is `search.messages`, which needs a *user* token
//! with `search:read`. Bot tokens can't search. Search sees everything the user
//! themself can; the query scopes it, not bot membership.

use std::collections::BTreeMap;
use std::collections::HashMap;

use super::{BriefIntegration, Connection, IntegrationItem};

const BASE: &str = "https://slack.com/api";

/// Cap on `users.list` pages fetched for mention resolution (200/page).
/// Beyond this, unresolved mentions degrade to raw ids — never an error.
const MAX_USER_PAGES: usize = 5;

/// Apply the headers every Slack request needs: bearer auth + a JSON accept.
fn with_headers(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    rb.bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
}

/// Slack wraps errors in HTTP-200 bodies: `{"ok": false, "error": "…"}`, so
/// `read_json`'s 401/403 mapping won't fire — every call checks `ok` after it
/// (same family as Linear's post-`read_json` GraphQL-error check). Maps the
/// error slug to a friendly message. Pure — unit-tested.
fn check_ok(json: &serde_json::Value) -> Result<(), String> {
    if json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(());
    }
    let slug = json.get("error").and_then(|v| v.as_str()).unwrap_or("");
    let msg = match slug {
        "invalid_auth" | "token_revoked" | "token_expired" | "account_inactive" => {
            "Slack rejected the credentials — check the connection in Settings.".to_string()
        }
        "missing_scope" => "Slack token is missing the search:read user scope — add it under your \
app's User Token Scopes and reinstall the app."
            .to_string(),
        "not_allowed_token_type" => "Slack search needs a user token (xoxp-…), not a bot token — \
copy the User OAuth Token from your app's OAuth & Permissions page."
            .to_string(),
        "ratelimited" => "Slack rate limit hit — try again in a minute.".to_string(),
        other => format!("Slack error: {other}"),
    };
    Err(msg)
}

/// Cheap credential check: hit `auth.test` with the bearer token. (It succeeds
/// even without `search:read` — a missing scope only surfaces on first fetch,
/// where `missing_scope` maps to the friendly message above. Acceptable.)
pub async fn validate(_conn: &Connection, token: &str) -> Result<(), String> {
    let resp = with_headers(super::http_client()?.post(format!("{BASE}/auth.test")), token)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let json = super::read_json(resp, "Slack").await?;
    check_ok(&json)
}

pub async fn fetch(
    _conn: &Connection,
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    match sel.kind.as_str() {
        "messages" => fetch_messages(sel, token).await,
        other => Err(format!("Slack supports kind: messages (got \"{other}\").")),
    }
}

/// One `search.messages` request, recency-ordered, mapped to items. Empty query
/// errors, since the per-brief query *is* the assignment (mirrors Gmail/Notion).
async fn fetch_messages(
    sel: &BriefIntegration,
    token: &str,
) -> Result<Vec<IntegrationItem>, String> {
    let q = sel
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or("Slack selector needs a search query (e.g. `in:#general after:2026-06-01`).")?;
    let max = sel.limit.unwrap_or(15).clamp(1, 50);

    let resp = with_headers(
        super::http_client()?.get(format!("{BASE}/search.messages")),
        token,
    )
    // Recency-ordered to match the panel's "what's happening lately" semantics
    // (Slack's default sort is relevance `score`).
    .query(&[
        ("query", q),
        ("count", &max.to_string()),
        ("sort", "timestamp"),
        ("sort_dir", "desc"),
    ])
    .send()
    .await
    .map_err(|e| format!("request failed: {e}"))?;
    let json = super::read_json(resp, "Slack").await?;
    check_ok(&json)?;

    let matches: Vec<&serde_json::Value> = json
        .pointer("/messages/matches")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default();

    // Only pay for the directory when a batch actually mentions someone.
    let needs_users = matches.iter().any(|m| {
        m.get("text")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t.contains("<@"))
    });
    let users = if needs_users {
        fetch_user_directory(token).await
    } else {
        HashMap::new()
    };

    let items = matches.iter().map(|m| map_match(m, &users)).collect();
    Ok(items)
}

/// Best-effort `users.list` → `{ user_id: display_name }` for mention
/// resolution. Returns an **empty map on any failure** (missing `users:read`,
/// rate-limit, network error) so resolution degrades to raw ids rather than
/// failing the fetch. Network — not unit-tested; the pure `collect_users` /
/// `pick_display_name` it feeds are.
async fn fetch_user_directory(token: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let client = match super::http_client() {
        Ok(c) => c,
        Err(_) => return map,
    };
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_USER_PAGES {
        let mut params = vec![("limit", "200".to_string())];
        if let Some(c) = &cursor {
            params.push(("cursor", c.clone()));
        }
        let resp = match with_headers(client.get(format!("{BASE}/users.list")), token)
            .query(&params)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => break,
        };
        let json = match super::read_json(resp, "Slack").await {
            Ok(j) => j,
            Err(_) => break,
        };
        if check_ok(&json).is_err() {
            break;
        }
        collect_users(&json, &mut map);
        match json
            .pointer("/response_metadata/next_cursor")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    map
}

/// Map one `search.messages` match into a normalized item. Pure — fixture-tested.
/// Never fabricates fields that aren't present.
fn map_match(m: &serde_json::Value, users: &HashMap<String, String>) -> IntegrationItem {
    let channel_id = m.pointer("/channel/id").and_then(|v| v.as_str());
    let channel_name = m.pointer("/channel/name").and_then(|v| v.as_str());
    let ts = m.get("ts").and_then(|v| v.as_str()).unwrap_or_default();

    let id = m
        .get("iid")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("{}:{}", channel_id.unwrap_or_default(), ts));

    let text = m.get("text").and_then(|v| v.as_str()).unwrap_or_default();
    let clean = clean_slack_text(text, users);

    let title = {
        let first = clean.lines().next().unwrap_or("").trim();
        if first.is_empty() {
            "(no text)".to_string()
        } else {
            first.chars().take(100).collect::<String>()
        }
    };

    let url = m
        .get("permalink")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "https://app.slack.com/client/{}/{}",
                m.get("team").and_then(|v| v.as_str()).unwrap_or_default(),
                channel_id.unwrap_or_default()
            )
        });

    let assignee = m
        .get("username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let mut meta = BTreeMap::new();
    if let Some(name) = channel_name.filter(|s| !s.is_empty()) {
        meta.insert("channel".to_string(), format!("#{name}"));
    }
    if !clean.is_empty() {
        meta.insert(
            "snippet".to_string(),
            clean.trim().chars().take(200).collect::<String>(),
        );
    }

    IntegrationItem {
        id,
        title,
        url,
        // Search results carry no read/unread signal — leave status empty.
        status: None,
        // The sender lives in the panel's "assignee" slot, same as Gmail's From.
        assignee,
        updated_at: ts_to_rfc3339(ts),
        kind: "messages".to_string(),
        meta,
    }
}

/// Parse Slack's `ts` (a string of **epoch seconds with a fractional suffix**,
/// `"1717372800.123456"`) into RFC3339. Take the integer-seconds part.
/// `summarize`'s "updated recently" math depends on a parseable RFC3339 here.
/// Pure — unit-tested. Sibling of `gmail.rs::internal_date_to_rfc3339`.
fn ts_to_rfc3339(ts: &str) -> Option<String> {
    let secs: i64 = ts.trim().split('.').next()?.parse().ok()?;
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

/// Minimal mrkdwn cleanup for display — no `users.info` lookups, no extra
/// requests. Resolves Slack's `<…>` entities and unescapes HTML entities, then
/// trims. Hand-rolled (no regex crate). Pure — unit-tested.
fn clean_slack_text(s: &str, users: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('>') {
            Some(end) => {
                out.push_str(&render_entity(&after[..end], users));
                rest = &after[end + 1..];
            }
            // Unterminated `<` — emit it literally and move on.
            None => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    unescape_entities(&out).trim().to_string()
}

/// Render the inside of one Slack `<…>` entity to display text. `users` maps a
/// Slack user id (`U…`) to a display name for mention resolution; an empty map
/// degrades gracefully to the inline name or the raw id.
fn render_entity(t: &str, users: &HashMap<String, String>) -> String {
    if let Some(rest) = t.strip_prefix('@') {
        // User mention `<@U123>` / `<@U123|name>`. Precedence: resolved display
        // name → inline `|name` → raw id.
        let (id, inline) = match rest.split_once('|') {
            Some((id, name)) => (id, Some(name)),
            None => (rest, None),
        };
        if let Some(name) = users.get(id).filter(|s| !s.is_empty()) {
            format!("@{name}")
        } else if let Some(name) = inline.filter(|s| !s.is_empty()) {
            format!("@{name}")
        } else {
            format!("@{id}")
        }
    } else if let Some(rest) = t.strip_prefix('#') {
        // Channel `<#C123|name>` → `#name`; `<#C123>` → `#C123`.
        match rest.split_once('|') {
            Some((_, name)) if !name.is_empty() => format!("#{name}"),
            _ => format!("#{rest}"),
        }
    } else if let Some(rest) = t.strip_prefix('!') {
        // Special `<!here>` → `@here`, `<!channel>`, `<!everyone>`.
        format!("@{}", rest.split('|').next().unwrap_or(rest))
    } else {
        // Link `<url|label>` → `label`; `<url>` → `url`.
        match t.split_once('|') {
            Some((_, label)) if !label.is_empty() => label.to_string(),
            _ => t.to_string(),
        }
    }
}

/// Unescape Slack's three HTML entities. `&amp;` is replaced last so a literal
/// `&amp;lt;` doesn't get double-unescaped.
fn unescape_entities(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Pull `{ id: display_name }` pairs out of one `users.list` page into `out`.
/// Pure — fixture-tested.
fn collect_users(json: &serde_json::Value, out: &mut HashMap<String, String>) {
    let Some(members) = json.get("members").and_then(|v| v.as_array()) else {
        return;
    };
    for m in members {
        if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
            if let Some(name) = pick_display_name(m) {
                out.insert(id.to_string(), name);
            }
        }
    }
}

/// Friendliest name for a Slack user object: `profile.display_name`, then
/// `profile.real_name`, then top-level `real_name`, then the legacy `name`
/// handle. Skips empties. Pure — unit-tested.
fn pick_display_name(m: &serde_json::Value) -> Option<String> {
    let prof = m.get("profile");
    let from_profile = |k: &str| {
        prof.and_then(|p| p.get(k))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };
    from_profile("display_name")
        .or_else(|| from_profile("real_name"))
        .or_else(|| m.get("real_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
        .or_else(|| m.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::from_str(
            r#"{
                "iid": "abc-123",
                "team": "T0123ABC",
                "channel": { "id": "C0456DEF", "name": "waid" },
                "type": "message",
                "user": "U0789GHI",
                "username": "dana",
                "ts": "1717372800.123456",
                "text": "Hey <@U222> — the <https://github.com/x/y|redesign PR> is up in <#C0456DEF|waid> &amp; ready",
                "permalink": "https://acme.slack.com/archives/C0456DEF/p1717372800123456"
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn maps_message() {
        let item = map_match(&sample(), &HashMap::new());
        assert_eq!(item.id, "abc-123");
        assert_eq!(
            item.title,
            "Hey @U222 — the redesign PR is up in #waid & ready"
        );
        assert_eq!(
            item.url,
            "https://acme.slack.com/archives/C0456DEF/p1717372800123456"
        );
        assert_eq!(item.assignee.as_deref(), Some("dana"));
        assert_eq!(item.status, None);
        assert_eq!(item.kind, "messages");
        assert_eq!(item.meta.get("channel").map(String::as_str), Some("#waid"));
        assert_eq!(
            item.meta.get("snippet").map(String::as_str),
            Some("Hey @U222 — the redesign PR is up in #waid & ready")
        );
        // ts parsed to a recognizable RFC3339 instant.
        assert_eq!(item.updated_at.as_deref(), Some("2024-06-03T00:00:00+00:00"));

        // With a populated directory, `<@U222>` resolves in both title and snippet.
        let mut users = HashMap::new();
        users.insert("U222".to_string(), "dana".to_string());
        let resolved = map_match(&sample(), &users);
        assert_eq!(
            resolved.title,
            "Hey @dana — the redesign PR is up in #waid & ready"
        );
        assert_eq!(
            resolved.meta.get("snippet").map(String::as_str),
            Some("Hey @dana — the redesign PR is up in #waid & ready")
        );
    }

    #[test]
    fn tolerates_missing_fields() {
        let m: serde_json::Value = serde_json::from_str(
            r#"{ "channel": { "id": "C9" }, "ts": "1717372800.000100" }"#,
        )
        .unwrap();
        let item = map_match(&m, &HashMap::new());
        // No iid → fallback id "{channel.id}:{ts}".
        assert_eq!(item.id, "C9:1717372800.000100");
        assert_eq!(item.title, "(no text)");
        assert_eq!(item.assignee, None);
        assert_eq!(item.status, None);
        // No permalink → app.slack.com fallback (empty team segment).
        assert_eq!(item.url, "https://app.slack.com/client//C9");
        assert!(item.meta.get("channel").is_none());
        assert!(item.meta.get("snippet").is_none());
    }

    #[test]
    fn ts_parses() {
        // 1717372800 s = 2024-06-03T00:00:00Z; fractional suffix is dropped.
        assert_eq!(
            ts_to_rfc3339("1717372800.123456").as_deref(),
            Some("2024-06-03T00:00:00+00:00")
        );
        assert_eq!(ts_to_rfc3339("not-a-ts"), None);
        // Round-trips through chrono's RFC3339 parser (what summarize uses).
        let rfc = ts_to_rfc3339("1717372800.123456").unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&rfc).is_ok());
    }

    #[test]
    fn clean_text() {
        let none = HashMap::new();
        assert_eq!(
            clean_slack_text("<https://example.com|click here>", &none),
            "click here"
        );
        assert_eq!(clean_slack_text("<https://example.com>", &none), "https://example.com");
        assert_eq!(clean_slack_text("<@U123ABC>", &none), "@U123ABC");
        // Inline-name fallback: `<@U123|dana>` renders `@dana` even with no directory.
        assert_eq!(clean_slack_text("<@U123|dana>", &none), "@dana");
        assert_eq!(clean_slack_text("<#C123|general>", &none), "#general");
        assert_eq!(clean_slack_text("<#C123>", &none), "#C123");
        assert_eq!(clean_slack_text("<!here>", &none), "@here");
        assert_eq!(clean_slack_text("<!channel>", &none), "@channel");
        assert_eq!(clean_slack_text("<!everyone>", &none), "@everyone");
        assert_eq!(clean_slack_text("a &amp; b &lt; c &gt; d", &none), "a & b < c > d");
        assert_eq!(clean_slack_text("  trim me  ", &none), "trim me");

        // A populated directory resolves a bare id, and a resolved name beats inline.
        let mut users = HashMap::new();
        users.insert("U123ABC".to_string(), "dana".to_string());
        assert_eq!(clean_slack_text("<@U123ABC>", &users), "@dana");
        let mut users2 = HashMap::new();
        users2.insert("U123".to_string(), "Dana K".to_string());
        assert_eq!(clean_slack_text("<@U123|dana>", &users2), "@Dana K");
    }

    #[test]
    fn picks_display_name() {
        // profile.display_name wins.
        let m = serde_json::json!({
            "profile": { "display_name": "dana", "real_name": "Dana Karan" },
            "real_name": "Dana K",
            "name": "dkaran"
        });
        assert_eq!(pick_display_name(&m).as_deref(), Some("dana"));

        // Falls back to profile.real_name when display_name is empty.
        let m = serde_json::json!({
            "profile": { "display_name": "", "real_name": "Dana Karan" }
        });
        assert_eq!(pick_display_name(&m).as_deref(), Some("Dana Karan"));

        // Then top-level real_name.
        let m = serde_json::json!({ "profile": {}, "real_name": "Dana K", "name": "dkaran" });
        assert_eq!(pick_display_name(&m).as_deref(), Some("Dana K"));

        // Then legacy name handle.
        let m = serde_json::json!({ "name": "dkaran" });
        assert_eq!(pick_display_name(&m).as_deref(), Some("dkaran"));

        // All-empty → None.
        let m = serde_json::json!({
            "profile": { "display_name": "", "real_name": "" },
            "real_name": "",
            "name": ""
        });
        assert_eq!(pick_display_name(&m), None);
    }

    #[test]
    fn collects_users() {
        let json = serde_json::json!({
            "members": [
                { "id": "U1", "profile": { "display_name": "dana" } },
                { "id": "U2", "name": "sam" },
                { "id": "U3", "profile": { "display_name": "" } }
            ]
        });
        let mut out = HashMap::new();
        collect_users(&json, &mut out);
        assert_eq!(out.get("U1").map(String::as_str), Some("dana"));
        assert_eq!(out.get("U2").map(String::as_str), Some("sam"));
        // No usable name → skipped.
        assert!(out.get("U3").is_none());

        // Missing `members` → no panic, leaves out untouched.
        let mut empty = HashMap::new();
        collect_users(&serde_json::json!({ "ok": true }), &mut empty);
        assert!(empty.is_empty());
    }

    #[test]
    fn check_ok_maps_errors() {
        assert!(check_ok(&serde_json::json!({ "ok": true })).is_ok());

        let cred = check_ok(&serde_json::json!({ "ok": false, "error": "invalid_auth" }));
        assert!(cred.unwrap_err().contains("rejected the credentials"));

        let scope = check_ok(&serde_json::json!({ "ok": false, "error": "missing_scope" }));
        assert!(scope.unwrap_err().contains("search:read"));

        let bot = check_ok(&serde_json::json!({ "ok": false, "error": "not_allowed_token_type" }));
        assert!(bot.unwrap_err().contains("user token"));

        let rl = check_ok(&serde_json::json!({ "ok": false, "error": "ratelimited" }));
        assert!(rl.unwrap_err().contains("rate limit"));

        let other = check_ok(&serde_json::json!({ "ok": false, "error": "weird_thing" }));
        assert_eq!(other.unwrap_err(), "Slack error: weird_thing");
    }
}
