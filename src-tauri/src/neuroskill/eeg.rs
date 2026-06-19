//! READ target #1: NeuroSkill's `eeg_timeseries` table (the ~5s-epoch history).
//!
//! Read-only. The only `SELECT` this module issues is [`EEG_QUERY`] against
//! `eeg_timeseries` — see the read-scope guard test in `super`. Each row's
//! `metrics` JSON blob is reduced to its top-level scalar fields by the pure
//! [`parse_metrics`] (per-channel band-power arrays and other non-scalars are
//! dropped).

use std::collections::BTreeMap;
use std::path::Path;

use super::aggregate::Epoch;

/// The sole query issued against `eeg_timeseries`. Read-only; references no other
/// table. Bounds the scan to one session interval.
pub const EEG_QUERY: &str =
    "SELECT ts, metrics FROM eeg_timeseries WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts";

/// Reduce a `metrics` JSON blob to its top-level numeric scalars. Non-numeric and
/// nested fields (`channels[]`, strings, objects) are dropped. Pure — unit-tested.
pub fn parse_metrics(json_str: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return out;
    };
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(n) = v.as_f64() {
                out.insert(k.clone(), n);
            }
        }
    }
    out
}

/// Read the EEG epochs in `[start, end]` from `activity.sqlite` (read-only).
pub fn read_epochs(db_path: &Path, start: i64, end: i64) -> Result<Vec<Epoch>, String> {
    let conn = super::open_ro(db_path)?;
    let mut stmt = conn
        .prepare(EEG_QUERY)
        .map_err(|e| format!("NeuroSkill eeg query failed: {e}"))?;
    let rows = stmt
        .query_map([start, end], |row| {
            let ts: i64 = row.get(0)?;
            let metrics: String = row.get(1)?;
            Ok((ts, metrics))
        })
        .map_err(|e| format!("NeuroSkill eeg read failed: {e}"))?;

    let mut epochs = Vec::new();
    for r in rows {
        let (ts, metrics) = r.map_err(|e| format!("NeuroSkill eeg row failed: {e}"))?;
        epochs.push(Epoch {
            ts,
            metrics: parse_metrics(&metrics),
        });
    }
    Ok(epochs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_top_level_scalars() {
        let json = r#"{
            "focus": 41.5, "engagement": 37, "mood": 62.0,
            "channels": [1, 2, 3],
            "label": "ignored-string",
            "nested": { "x": 1 },
            "timestamp": 1717372800.5
        }"#;
        let m = parse_metrics(json);
        assert_eq!(m.get("focus"), Some(&41.5));
        assert_eq!(m.get("engagement"), Some(&37.0));
        assert_eq!(m.get("mood"), Some(&62.0));
        // timestamp is a scalar so it's parsed here; aggregate() drops it later.
        assert_eq!(m.get("timestamp"), Some(&1717372800.5));
        // Non-scalars dropped.
        assert!(m.get("channels").is_none());
        assert!(m.get("label").is_none());
        assert!(m.get("nested").is_none());
    }

    #[test]
    fn tolerates_garbage_json() {
        assert!(parse_metrics("not json").is_empty());
        assert!(parse_metrics("").is_empty());
        assert!(parse_metrics("[1,2,3]").is_empty()); // array, not an object
    }
}
