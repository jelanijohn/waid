//! Pure, fixture-tested rollup core: label events → session intervals, the
//! rolling-window filter, and (intervals + EEG epochs) → a `MindState`.
//!
//! All I/O (SQLite reads, the clock) lives at the edges (`labels.rs`, `eeg.rs`,
//! `mod.rs`); everything here is deterministic given its inputs, so the whole
//! aggregation contract is unit-testable.

use std::collections::{BTreeMap, BTreeSet};

/// Hard cap on a single session's length, used to close a `:start` with no
/// matching `:end` (a forgotten end can't run away and skew the rollup).
pub const MAX_SESSION_SECS: i64 = 4 * 3600;

/// Trend threshold (points on the 0–100 metric scale): second-half mean must beat
/// first-half by more than this to read as ↑/↓, else →.
const TREND_EPS: f64 = 2.0;

/// The metrics rendered in the region table, in order: `(json_key, display)`.
/// The rollup computes means/trends for *every* tracked metric; the renderer
/// picks this subset. `focus` also drives the "deepest focus" peak block.
pub const DISPLAY_METRICS: &[(&str, &str)] = &[
    ("focus", "Focus"),
    ("engagement", "Engagement"),
    ("mood", "Mood"),
    ("relaxation", "Relaxation"),
];

/// Rolling-window spec from the selector `query`. Defaults to 14 days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Today,
    D7,
    D14,
    D30,
}

impl Window {
    /// Parse a single window token; anything unrecognised falls back to `D14`.
    pub fn parse(s: &str) -> Window {
        match s.trim().to_ascii_lowercase().as_str() {
            "today" => Window::Today,
            "7d" => Window::D7,
            "30d" => Window::D30,
            _ => Window::D14,
        }
    }

    /// The lower bound of the rolling window, in unix seconds. `Today` is from the
    /// start of the current UTC day; the rest are `now - N*86400`.
    pub fn cutoff(&self, now: i64) -> i64 {
        match self {
            Window::Today => now - now.rem_euclid(86400),
            Window::D7 => now - 7 * 86400,
            Window::D14 => now - 14 * 86400,
            Window::D30 => now - 30 * 86400,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Window::Today => "Today",
            Window::D7 => "Rolling 7 days",
            Window::D14 => "Rolling 14 days",
            Window::D30 => "Rolling 30 days",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    End,
}

/// One parsed `waid:brief=<slug>:(start|end)` label, with its wall-clock time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabelEvent {
    pub phase: Phase,
    pub wall: i64,
}

/// A closed work session window, unix seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub start: i64,
    pub end: i64,
}

/// One ~5s EEG epoch: its timestamp plus the top-level scalar metrics parsed from
/// the `metrics` JSON blob (non-scalar fields like `channels[]` already dropped).
#[derive(Clone, Debug, PartialEq)]
pub struct Epoch {
    pub ts: i64,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    Up,
    Flat,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricRoll {
    pub mean: f64,
    pub trend: Trend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakBlock {
    pub start: i64,
    pub end: i64,
    pub focus: f64,
}

/// The deterministic rollup the renderer formats. Carries raw unix timestamps for
/// the peak block; clock-time formatting (timezone-dependent) happens at render.
#[derive(Clone, Debug, PartialEq)]
pub struct MindState {
    pub window_label: &'static str,
    pub n_sessions: usize,
    pub total_tracked_secs: i64,
    pub n_epochs: usize,
    /// Mean + trend per tracked metric (all of them; renderer picks a subset).
    pub metrics: BTreeMap<String, MetricRoll>,
    pub peak: Option<PeakBlock>,
}

impl MindState {
    pub fn is_empty(&self) -> bool {
        self.n_epochs == 0
    }
}

/// Pair `:start`/`:end` events into intervals. Events are sorted by wall time.
/// A `:start` while one is already open closes the previous at the cap (forgotten
/// end); a `:start` with no `:end` at all closes at `min(start+cap, now)`. A
/// `:end` before any open `:start`, or before its start, is ignored.
pub fn pair_intervals(mut events: Vec<LabelEvent>, now: i64) -> Vec<Interval> {
    events.sort_by_key(|e| e.wall);
    let mut out = Vec::new();
    let mut open: Option<i64> = None;
    for e in events {
        match e.phase {
            Phase::Start => {
                if let Some(s) = open {
                    // Two starts with no end between them: cap the first, bounded
                    // by the new start and now.
                    let end = (s + MAX_SESSION_SECS).min(e.wall).min(now).max(s);
                    out.push(Interval { start: s, end });
                }
                open = Some(e.wall);
            }
            Phase::End => {
                if let Some(s) = open.take() {
                    if e.wall >= s {
                        out.push(Interval { start: s, end: e.wall });
                    }
                    // else: stray end before its start — drop it.
                }
            }
        }
    }
    if let Some(s) = open {
        out.push(Interval {
            start: s,
            end: (s + MAX_SESSION_SECS).min(now).max(s),
        });
    }
    out
}

/// Keep only intervals whose end is within the rolling window.
pub fn within_window(intervals: &[Interval], cutoff: i64) -> Vec<Interval> {
    intervals.iter().copied().filter(|iv| iv.end >= cutoff).collect()
}

fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

fn trend_of(first: &[f64], second: &[f64]) -> Trend {
    match (mean(first), mean(second)) {
        (Some(a), Some(b)) => {
            let d = b - a;
            if d > TREND_EPS {
                Trend::Up
            } else if d < -TREND_EPS {
                Trend::Down
            } else {
                Trend::Flat
            }
        }
        _ => Trend::Flat,
    }
}

/// Roll up sessions (each an interval + its collected epochs) into a `MindState`.
/// Means/trends are over all epochs in the window; the trend splits epochs at the
/// window midpoint (first half vs second half). The peak block is the session
/// with the highest mean `focus`.
pub fn aggregate(window: Window, sessions: &[(Interval, Vec<Epoch>)], now: i64) -> MindState {
    let cutoff = window.cutoff(now);
    let mid = cutoff + (now - cutoff) / 2;

    let n_sessions = sessions.len();
    let total_tracked_secs: i64 = sessions.iter().map(|(iv, _)| (iv.end - iv.start).max(0)).sum();

    let all: Vec<&Epoch> = sessions.iter().flat_map(|(_, e)| e.iter()).collect();
    let n_epochs = all.len();

    let mut keys: BTreeSet<&str> = BTreeSet::new();
    for e in &all {
        for k in e.metrics.keys() {
            // `timestamp` is a float echo of `ts`, not a real metric — skip it.
            if k != "timestamp" {
                keys.insert(k.as_str());
            }
        }
    }

    let mut metrics = BTreeMap::new();
    for k in keys {
        let vals: Vec<f64> = all.iter().filter_map(|e| e.metrics.get(k).copied()).collect();
        let Some(m) = mean(&vals) else { continue };
        let first: Vec<f64> = all
            .iter()
            .filter(|e| e.ts < mid)
            .filter_map(|e| e.metrics.get(k).copied())
            .collect();
        let second: Vec<f64> = all
            .iter()
            .filter(|e| e.ts >= mid)
            .filter_map(|e| e.metrics.get(k).copied())
            .collect();
        metrics.insert(
            k.to_string(),
            MetricRoll {
                mean: m,
                trend: trend_of(&first, &second),
            },
        );
    }

    let mut peak: Option<PeakBlock> = None;
    for (iv, eps) in sessions {
        let focuses: Vec<f64> = eps.iter().filter_map(|e| e.metrics.get("focus").copied()).collect();
        let Some(f) = mean(&focuses) else { continue };
        if peak.as_ref().map(|p| f > p.focus).unwrap_or(true) {
            peak = Some(PeakBlock {
                start: iv.start,
                end: iv.end,
                focus: f,
            });
        }
    }

    MindState {
        window_label: window.label(),
        n_sessions,
        total_tracked_secs,
        n_epochs,
        metrics,
        peak,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(phase: Phase, wall: i64) -> LabelEvent {
        LabelEvent { phase, wall }
    }

    fn epoch(ts: i64, focus: f64, mood: f64) -> Epoch {
        let mut m = BTreeMap::new();
        m.insert("focus".to_string(), focus);
        m.insert("mood".to_string(), mood);
        Epoch { ts, metrics: m }
    }

    #[test]
    fn pairs_simple_session() {
        let now = 10_000;
        let ivs = pair_intervals(vec![ev(Phase::Start, 100), ev(Phase::End, 700)], now);
        assert_eq!(ivs, vec![Interval { start: 100, end: 700 }]);
    }

    #[test]
    fn pairs_out_of_order_rows() {
        // Rows arrive end-before-start in input order; sorting by wall fixes it.
        let ivs = pair_intervals(vec![ev(Phase::End, 700), ev(Phase::Start, 100)], 10_000);
        assert_eq!(ivs, vec![Interval { start: 100, end: 700 }]);
    }

    #[test]
    fn unclosed_start_is_capped() {
        // Start with no end → capped at start + MAX_SESSION_SECS (well before now).
        let now = 100 + MAX_SESSION_SECS + 50_000;
        let ivs = pair_intervals(vec![ev(Phase::Start, 100)], now);
        assert_eq!(ivs, vec![Interval { start: 100, end: 100 + MAX_SESSION_SECS }]);
    }

    #[test]
    fn unclosed_start_capped_at_now_when_recent() {
        // Start with no end but now is sooner than the cap → close at now.
        let ivs = pair_intervals(vec![ev(Phase::Start, 100)], 400);
        assert_eq!(ivs, vec![Interval { start: 100, end: 400 }]);
    }

    #[test]
    fn double_start_closes_previous_at_new_start() {
        let now = 10_000;
        let ivs = pair_intervals(
            vec![ev(Phase::Start, 100), ev(Phase::Start, 500), ev(Phase::End, 900)],
            now,
        );
        assert_eq!(
            ivs,
            vec![
                Interval { start: 100, end: 500 },
                Interval { start: 500, end: 900 },
            ]
        );
    }

    #[test]
    fn stray_end_is_ignored() {
        let ivs = pair_intervals(vec![ev(Phase::End, 500), ev(Phase::End, 600)], 10_000);
        assert!(ivs.is_empty());
    }

    #[test]
    fn rolling_window_boundary() {
        let now = 100 * 86400;
        let cutoff = Window::D7.cutoff(now); // now - 7d
        let intervals = vec![
            Interval { start: cutoff - 100, end: cutoff },      // ends exactly at cutoff → kept
            Interval { start: cutoff - 200, end: cutoff - 1 },  // ends just before → dropped
            Interval { start: now - 3600, end: now },           // recent → kept
        ];
        let kept = within_window(&intervals, cutoff);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|iv| iv.end >= cutoff));
    }

    #[test]
    fn aggregates_means_trend_and_peak() {
        // now far enough out that the 14d window covers all epochs; midpoint splits
        // the two sessions cleanly.
        let now = 1_000_000;
        let cutoff = Window::D14.cutoff(now);
        let mid = cutoff + (now - cutoff) / 2;

        // Session 1 (before mid): low focus. Session 2 (after mid): high focus.
        let iv1 = Interval { start: mid - 2000, end: mid - 1000 };
        let iv2 = Interval { start: mid + 1000, end: mid + 2000 };
        let s1 = (iv1, vec![epoch(iv1.start, 30.0, 50.0), epoch(iv1.start + 5, 34.0, 50.0)]);
        let s2 = (iv2, vec![epoch(iv2.start, 60.0, 50.0), epoch(iv2.start + 5, 64.0, 50.0)]);

        let ms = aggregate(Window::D14, &[s1, s2], now);
        assert_eq!(ms.n_sessions, 2);
        assert_eq!(ms.n_epochs, 4);
        assert_eq!(ms.total_tracked_secs, 2000);

        let focus = ms.metrics.get("focus").unwrap();
        assert!((focus.mean - 47.0).abs() < 1e-9); // (30+34+60+64)/4
        assert_eq!(focus.trend, Trend::Up); // second half (62) >> first half (32)

        let mood = ms.metrics.get("mood").unwrap();
        assert_eq!(mood.trend, Trend::Flat); // unchanged across halves

        // `timestamp` is never surfaced as a metric.
        assert!(ms.metrics.get("timestamp").is_none());

        // Peak is the high-focus session.
        let peak = ms.peak.unwrap();
        assert_eq!((peak.start, peak.end), (iv2.start, iv2.end));
        assert!((peak.focus - 62.0).abs() < 1e-9);
    }

    #[test]
    fn empty_when_no_epochs() {
        let ms = aggregate(Window::D14, &[], 1_000_000);
        assert!(ms.is_empty());
        assert_eq!(ms.n_sessions, 0);
        assert_eq!(ms.total_tracked_secs, 0);
        assert!(ms.peak.is_none());
        assert!(ms.metrics.is_empty());
    }

    #[test]
    fn window_parse_and_label() {
        assert_eq!(Window::parse("today"), Window::Today);
        assert_eq!(Window::parse("7d"), Window::D7);
        assert_eq!(Window::parse("14d"), Window::D14);
        assert_eq!(Window::parse("30d"), Window::D30);
        assert_eq!(Window::parse("garbage"), Window::D14); // default
        assert_eq!(Window::D14.label(), "Rolling 14 days");
    }
}
