//! The single **write** to NeuroSkill: fire its local `label` command.
//!
//! The daemon exposes the same command set over a WebSocket and a plain HTTP
//! `POST /` endpoint, both gated by a bearer token it writes to
//! `<config>/skill/daemon/auth.token`. WAID uses the HTTP transport — simpler and
//! more robust than a hand-rolled WS upgrade — via `reqwest` (the same client the
//! `provider/` modules use). We never write to `labels.sqlite` directly; the
//! daemon owns that file and the `label` command is the sanctioned path.
//!
//! Like Slack/Linear, the daemon can report a command-level failure as HTTP 200
//! with `{"ok": false}`, so we check the body as well as the status code.

use serde_json::Value;

/// POST `{"command":"label","text":<text>}` to the daemon. `base` is the HTTP
/// origin (e.g. `http://127.0.0.1:18444`); `token`, when present, is sent as a
/// bearer credential (the daemon rejects unauthenticated calls with 401).
/// Best-effort: any failure returns a friendly `Err` the caller surfaces as a
/// toast and never blocks the launch on it.
pub async fn fire_label(base: &str, token: Option<&str>, text: &str) -> Result<(), String> {
    let url = format!("{}/", base.trim_end_matches('/'));
    let mut req = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "command": "label", "text": text }));
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("NeuroSkill isn't reachable at {url} (is the daemon running?): {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(
            "NeuroSkill rejected the request (401) — the daemon auth token is missing or wrong. \
             Check the connection's token path."
                .to_string(),
        );
    }
    if !status.is_success() {
        return Err(format!("NeuroSkill returned HTTP {}.", status.as_u16()));
    }

    // 200 OK still doesn't guarantee success — the daemon reports command-level
    // failure in-band as `{"ok": false, ...}`.
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("NeuroSkill sent an unreadable response: {e}"))?;
    if body.get("ok").and_then(Value::as_bool) == Some(false) {
        let err = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(format!("NeuroSkill couldn't record the label: {err}."));
    }
    Ok(())
}
