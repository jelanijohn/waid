//! READ target #2: NeuroSkill's `labels` table, the attribution join key.
//!
//! Read-only. The only `SELECT` this module issues is [`LABELS_QUERY`] against the
//! `labels` table — see the read-scope guard test in `super`. We pull every
//! label's `(text, wall_start)` and keep only the WAID-authored
//! `waid:brief=<slug>:(start|end)` markers, parsed by the pure [`parse_label`].

use std::path::Path;

use super::aggregate::{LabelEvent, Phase};

/// The sole query issued against `labels`. Read-only; references no other table.
pub const LABELS_QUERY: &str =
    "SELECT text, wall_start FROM labels WHERE text IS NOT NULL AND wall_start IS NOT NULL";

/// Match a label's text against this brief's `waid:brief=<slug>:(start|end)`
/// marker. Pure — unit-tested. Only WAID's own marker format is recognised; any
/// other label text (calibration, the user's own labels) is ignored, so raw
/// label text never influences the rollup.
pub fn parse_label(text: &str, slug: &str) -> Option<Phase> {
    let t = text.trim();
    let prefix = format!("waid:brief={slug}:");
    match t.strip_prefix(&prefix)? {
        "start" => Some(Phase::Start),
        "end" => Some(Phase::End),
        _ => None,
    }
}

/// Read this brief's session-marker events from `labels.sqlite` (read-only).
pub fn read_label_events(db_path: &Path, slug: &str) -> Result<Vec<LabelEvent>, String> {
    let conn = super::open_ro(db_path)?;
    let mut stmt = conn
        .prepare(LABELS_QUERY)
        .map_err(|e| format!("NeuroSkill labels query failed: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let text: String = row.get(0)?;
            let wall: i64 = row.get(1)?;
            Ok((text, wall))
        })
        .map_err(|e| format!("NeuroSkill labels read failed: {e}"))?;

    let mut events = Vec::new();
    for r in rows {
        let (text, wall) = r.map_err(|e| format!("NeuroSkill labels row failed: {e}"))?;
        if let Some(phase) = parse_label(&text, slug) {
            events.push(LabelEvent { phase, wall });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waid_markers_only() {
        assert_eq!(parse_label("waid:brief=solaris:start", "solaris"), Some(Phase::Start));
        assert_eq!(parse_label("waid:brief=solaris:end", "solaris"), Some(Phase::End));
        // Whitespace tolerated.
        assert_eq!(parse_label("  waid:brief=solaris:start  ", "solaris"), Some(Phase::Start));
        // Different slug → not ours.
        assert_eq!(parse_label("waid:brief=other:start", "solaris"), None);
        // Calibration / arbitrary labels → ignored (never echoed into the brief).
        assert_eq!(parse_label("Eyes Open", "solaris"), None);
        assert_eq!(parse_label("waid:brief=solaris:pause", "solaris"), None);
    }
}
