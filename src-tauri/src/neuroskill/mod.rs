//! NeuroSkill integration — the deterministic `## Mind State` body region.
//!
//! This module is deliberately **outside** `provider/`: that module is documented
//! pure/network-only (never touches disk), whereas NeuroSkill reads **local
//! SQLite**. It reads two tables read-only (`eeg_timeseries` + `labels`, the only
//! `SELECT`s issued — see the read-scope guard test) and performs exactly one
//! write, the `label` WebSocket command (`ws.rs`). It never links or vendors any
//! NeuroSkill/GPL code — the integration lives strictly at the process/file
//! boundary.
//!
//! Pipeline (mirrors `## Activity`): read labels → pair into session intervals →
//! filter to the rolling window → read EEG epochs per interval → deterministic
//! rollup (`aggregate`, the fixture-tested core) → render markdown → the caller
//! upserts it into the `waid:mind` marked block. No LLM, no nondeterminism.

pub mod aggregate;
pub mod eeg;
pub mod labels;
pub mod ws;

use std::path::{Path, PathBuf};

use crate::provider::Connection;
pub use aggregate::{MindState, Window};

/// Default NeuroSkill WebSocket endpoint (localhost, fixed port; not the public
/// docs' `0.0.0.0`/random-port). Overridable per connection for WSL2↔host.
pub const DEFAULT_WS_URL: &str = "ws://127.0.0.1:8375";

/// Default data directory: the WSL2-translated Windows AppData path. Overridable
/// per connection (the same Windows-host/WSL2 split handled for Gmail OAuth).
pub const DEFAULT_DATA_DIR: &str = "/mnt/c/Users/jelan/AppData/Local/NeuroSkill";

/// Open a NeuroSkill SQLite file **read-only and immutable** — a static snapshot
/// of the main db. `immutable=1` means we never need write access to the `-shm`
/// and never contend with the always-on daemon's write lock; the tradeoff is that
/// writes still sitting in the `-wal` (not yet checkpointed) aren't seen, which is
/// fine for a manually-refreshed, multi-day rolling aggregate.
pub(crate) fn open_ro(db_path: &Path) -> Result<rusqlite::Connection, String> {
    use rusqlite::OpenFlags;
    if !db_path.exists() {
        return Err(format!(
            "NeuroSkill data file not found: {} — check the connection's data directory.",
            db_path.display()
        ));
    }
    let uri = format!("file:{}?mode=ro&immutable=1", db_path.to_string_lossy());
    rusqlite::Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("could not open {} read-only: {e}", db_path.display()))
}

/// The WebSocket endpoint for a NeuroSkill connection (override or default).
pub fn ws_url(conn: &Connection) -> String {
    conn.ws_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_WS_URL)
        .to_string()
}

/// The data directory for a NeuroSkill connection (override or default).
pub fn data_dir(conn: &Connection) -> PathBuf {
    conn.data_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

/// Parse a `mind` selector's `query` into `(window, slug)`. Recognises a window
/// token (`today|7d|14d|30d`, default 14d) and an optional `slug:<name>` override;
/// the slug defaults to the brief's own slug. Pure — unit-tested.
pub fn parse_mind_query(query: Option<&str>, default_slug: &str) -> (Window, String) {
    let mut window = Window::D14;
    let mut slug = default_slug.to_string();
    if let Some(q) = query {
        for tok in q.split_whitespace() {
            if let Some(s) = tok.strip_prefix("slug:") {
                if !s.is_empty() {
                    slug = s.to_string();
                }
            } else if matches!(tok.to_ascii_lowercase().as_str(), "today" | "7d" | "14d" | "30d") {
                window = Window::parse(tok);
            }
        }
    }
    (window, slug)
}

/// Compute the rolling-window rollup for one brief: read labels → pair intervals →
/// window-filter → read EEG epochs → aggregate. All reads are read-only; `now` is
/// passed in (unix seconds) so the orchestration stays deterministic given the DB.
pub fn compute_mind_state(
    data_dir: &Path,
    slug: &str,
    window: Window,
    now: i64,
) -> Result<MindState, String> {
    let labels_db = data_dir.join("labels.sqlite");
    let activity_db = data_dir.join("activity.sqlite");

    let events = labels::read_label_events(&labels_db, slug)?;
    let intervals = aggregate::pair_intervals(events, now);
    let cutoff = window.cutoff(now);
    let kept = aggregate::within_window(&intervals, cutoff);

    let mut sessions = Vec::with_capacity(kept.len());
    for iv in kept {
        let eps = eeg::read_epochs(&activity_db, iv.start, iv.end)?;
        sessions.push((iv, eps));
    }
    Ok(aggregate::aggregate(window, &sessions, now))
}

/// Fire one `label` WebSocket command (the only write). Best-effort.
pub async fn fire_session_label(ws_url: &str, text: &str) -> Result<(), String> {
    ws::fire_label(ws_url, text).await
}

/// Render the `## Mind State` region body (numbers + WAID-formatted times only —
/// never raw label/window/file text, keeping the brief portable). The `## Mind
/// State` heading lives **inside** the block so it regenerates with it (same shape
/// as `render_sync_summary`'s `## Activity`). `now` is unix seconds.
pub fn render_mind_state(ms: &MindState, now: i64) -> String {
    use aggregate::DISPLAY_METRICS;

    let updated = fmt_date(now);
    if ms.is_empty() {
        return format!(
            "## Mind State\n\n_{} · no labeled sessions with EEG data yet · updated {}_\n\n\
Launch this project to start a labeled session.",
            ms.window_label, updated
        );
    }

    let hrs = ms.total_tracked_secs as f64 / 3600.0;
    let mut out = String::from("## Mind State\n\n");
    out.push_str(&format!(
        "_{} · {} session{} · {:.1} hrs tracked · updated {}_\n\n",
        ms.window_label,
        ms.n_sessions,
        if ms.n_sessions == 1 { "" } else { "s" },
        hrs,
        updated,
    ));
    out.push_str("| Metric        | Mean | Trend |\n");
    out.push_str("|---------------|-----:|:-----:|\n");
    for (key, label) in DISPLAY_METRICS {
        if let Some(r) = ms.metrics.get(*key) {
            out.push_str(&format!(
                "| {:<13} | {:>4} | {:^5} |\n",
                label,
                r.mean.round() as i64,
                arrow(r.trend),
            ));
        }
    }
    if let Some(p) = &ms.peak {
        out.push_str(&format!(
            "\nDeepest focus: {} (focus {}).",
            fmt_block(p.start, p.end),
            p.focus.round() as i64,
        ));
    }
    out
}

/// A compact one-paragraph summary of the rollup for the synthesis evidence pool
/// (Phase 2). Numbers only — never raw label text — so it stays portable and
/// can't carry injected instructions; it spells trends in words for the model and
/// flags itself as descriptive, not directive. `None` when there's no signal
/// (empty window), so synthesis evidence isn't padded with an empty state.
pub fn evidence_text(ms: &MindState) -> Option<String> {
    use aggregate::{Trend, DISPLAY_METRICS};
    if ms.is_empty() {
        return None;
    }
    let hrs = ms.total_tracked_secs as f64 / 3600.0;
    let mut metric_parts: Vec<String> = Vec::new();
    for (key, label) in DISPLAY_METRICS {
        if let Some(r) = ms.metrics.get(*key) {
            let dir = match r.trend {
                Trend::Up => "rising",
                Trend::Flat => "steady",
                Trend::Down => "falling",
            };
            metric_parts.push(format!("{label} {} ({dir})", r.mean.round() as i64));
        }
    }
    let peak = ms
        .peak
        .as_ref()
        .map(|p| format!(" Deepest-focus session averaged {}.", p.focus.round() as i64))
        .unwrap_or_default();
    Some(format!(
        "{} · {} session{} · {:.1} hrs of EEG-tracked focused work. \
Mean scores (0–100): {}.{} This is a descriptive readout of the maintainer's \
measured focus/engagement/mood over their labeled work sessions — context, not a directive.",
        ms.window_label,
        ms.n_sessions,
        if ms.n_sessions == 1 { "" } else { "s" },
        hrs,
        metric_parts.join(", "),
        peak,
    ))
}

fn arrow(t: aggregate::Trend) -> &'static str {
    match t {
        aggregate::Trend::Up => "↑",
        aggregate::Trend::Flat => "→",
        aggregate::Trend::Down => "↓",
    }
}

/// Format a unix instant as a local `YYYY-MM-DD` date. Timezone-dependent, so it
/// lives here (impure) rather than in the fixture-tested `aggregate`.
fn fmt_date(now: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(now, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => "—".to_string(),
    }
}

/// Format a session interval as a local `Wkd 2pm–3pm` clock block.
fn fmt_block(start: i64, end: i64) -> String {
    use chrono::{Local, TimeZone};
    let h = |ts: i64| {
        Local
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%-I%p").to_string().to_lowercase())
    };
    match (Local.timestamp_opt(start, 0).single(), h(start), h(end)) {
        (Some(s), Some(hs), Some(he)) => format!("{} {}–{}", s.format("%a"), hs, he),
        _ => "—".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mind_query() {
        // Default window + default slug.
        assert_eq!(parse_mind_query(None, "solaris"), (Window::D14, "solaris".to_string()));
        // Window only.
        assert_eq!(parse_mind_query(Some("7d"), "solaris"), (Window::D7, "solaris".to_string()));
        // Window + slug override, any order.
        assert_eq!(
            parse_mind_query(Some("slug:alt 30d"), "solaris"),
            (Window::D30, "alt".to_string())
        );
        assert_eq!(
            parse_mind_query(Some("today slug:alt"), "solaris"),
            (Window::Today, "alt".to_string())
        );
        // Junk tokens ignored, default window kept.
        assert_eq!(parse_mind_query(Some("garbage"), "solaris"), (Window::D14, "solaris".to_string()));
    }

    #[test]
    fn renders_empty_state() {
        let ms = aggregate::aggregate(Window::D14, &[], 1_000_000);
        let region = render_mind_state(&ms, 1_000_000);
        assert!(region.starts_with("## Mind State"));
        assert!(region.contains("no labeled sessions"));
        // Never any table rows when empty.
        assert!(!region.contains("| Focus"));
    }

    #[test]
    fn renders_populated_table() {
        use aggregate::{MetricRoll, MindState, PeakBlock, Trend};
        use std::collections::BTreeMap;
        let mut metrics = BTreeMap::new();
        metrics.insert("focus".to_string(), MetricRoll { mean: 41.4, trend: Trend::Up });
        metrics.insert("mood".to_string(), MetricRoll { mean: 62.0, trend: Trend::Flat });
        let ms = MindState {
            window_label: "Rolling 14 days",
            n_sessions: 4,
            total_tracked_secs: 11_520, // 3.2 hrs
            n_epochs: 100,
            metrics,
            peak: Some(PeakBlock { start: 1_717_000_000, end: 1_717_003_600, focus: 58.0 }),
        };
        let region = render_mind_state(&ms, 1_717_000_000);
        assert!(region.contains("Rolling 14 days · 4 sessions · 3.2 hrs tracked"));
        assert!(region.contains("| Focus")); // rounded mean 41
        assert!(region.contains("41")); // mean, right-padded in the cell
        assert!(region.contains("| Mood"));
        assert!(region.contains("↑"));
        assert!(region.contains("Deepest focus:"));
        assert!(region.contains("(focus 58)."));
        // Engagement/Relaxation absent from this fixture → not rendered.
        assert!(!region.contains("| Engagement"));
    }

    #[test]
    fn evidence_text_summarizes_or_none() {
        use aggregate::{MetricRoll, MindState, PeakBlock, Trend};
        use std::collections::BTreeMap;

        // Empty window → no evidence (don't pad the synthesis pool).
        let empty = aggregate::aggregate(Window::D14, &[], 1_000_000);
        assert!(evidence_text(&empty).is_none());

        let mut metrics = BTreeMap::new();
        metrics.insert("focus".to_string(), MetricRoll { mean: 41.0, trend: Trend::Up });
        metrics.insert("mood".to_string(), MetricRoll { mean: 62.0, trend: Trend::Down });
        let ms = MindState {
            window_label: "Rolling 14 days",
            n_sessions: 4,
            total_tracked_secs: 11_520,
            n_epochs: 100,
            metrics,
            peak: Some(PeakBlock { start: 0, end: 3600, focus: 58.0 }),
        };
        let text = evidence_text(&ms).unwrap();
        assert!(text.contains("Rolling 14 days · 4 sessions · 3.2 hrs"));
        assert!(text.contains("Focus 41 (rising)"));
        assert!(text.contains("Mood 62 (falling)"));
        assert!(text.contains("Deepest-focus session averaged 58"));
        // Framed as descriptive context, not an instruction.
        assert!(text.contains("not a directive"));
    }
}

/// Read-scope guard: the only `SELECT`s this module ever issues are the two query
/// constants, and they touch only `eeg_timeseries` + `labels`. This is what makes
/// the user's `track_*` toggles harmless and keeps window/file/terminal titles out
/// of portable briefs (charter §7).
#[cfg(test)]
mod read_scope_guard {
    use super::eeg::EEG_QUERY;
    use super::labels::LABELS_QUERY;

    /// Every table the integration must NEVER read (spec §2.5).
    const FORBIDDEN_TABLES: &[&str] = &[
        "active_windows",
        "secondary_windows",
        "terminal_sessions",
        "terminal_commands",
        "terminal_outputs",
        "browser_activities",
        "clipboard_events",
        "user_screenshot_events",
        "file_interactions",
        "file_edit_chunks",
        "meeting_events",
        "conversations",
        "zone_switches",
        "input_",
        "ai_events",
        "brain_feedback",
        "focus_sessions",
        "embeddings",
    ];

    #[test]
    fn queries_are_read_only_and_scoped() {
        for q in [EEG_QUERY, LABELS_QUERY] {
            let lower = q.to_lowercase();
            assert!(lower.trim_start().starts_with("select"), "not a SELECT: {q}");
            for kw in ["insert", "update ", "delete", "drop", "attach", "pragma", "create"] {
                assert!(!lower.contains(kw), "non-read keyword `{kw}` in: {q}");
            }
            for t in FORBIDDEN_TABLES {
                assert!(!lower.contains(t), "forbidden table `{t}` in: {q}");
            }
        }
        assert!(EEG_QUERY.contains("eeg_timeseries"));
        assert!(LABELS_QUERY.contains("labels"));
    }
}
