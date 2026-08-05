//! The two alert conditions this app watches for, plus an edge-triggered
//! notification engine so each condition fires once on the way up, escalates
//! if it gets worse, periodically re-notifies while it stays critical, and
//! announces recovery — instead of spamming a notification per feed message.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::state::PeerDrop;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum Severity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AlertEvent {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub resolved: bool,
}

/// User-facing thresholds for this app: last block > 6s, and a validator's
/// peer count halving (see state::peer_drop_candidates) within ~90s.
pub const BLOCK_STALL_WARN_SECS: f64 = 6.0;
pub const BLOCK_STALL_CRITICAL_SECS: f64 = 20.0;
const RENOTIFY_COOLDOWN: Duration = Duration::from_secs(10 * 60);

fn build_active_alerts(
    seconds_since_last_block: f64,
    peer_drops: &[PeerDrop],
) -> Vec<(String, Severity, String, String)> {
    let mut alerts = Vec::new();

    if seconds_since_last_block > BLOCK_STALL_CRITICAL_SECS {
        alerts.push((
            "block-stall".to_string(),
            Severity::Critical,
            "Block production stalled".to_string(),
            format!(
                "No new block in {:.0}s (critical threshold {:.0}s)",
                seconds_since_last_block, BLOCK_STALL_CRITICAL_SECS
            ),
        ));
    } else if seconds_since_last_block > BLOCK_STALL_WARN_SECS {
        alerts.push((
            "block-stall".to_string(),
            Severity::Warning,
            "Block time elevated".to_string(),
            format!(
                "No new block in {:.1}s (>{:.0}s)",
                seconds_since_last_block, BLOCK_STALL_WARN_SECS
            ),
        ));
    }

    for (id, name, baseline, current) in peer_drops {
        let severity = if *current == 0 { Severity::Critical } else { Severity::Warning };
        alerts.push((
            format!("peer-drop-{id}"),
            severity,
            format!("{name}: peers dropped"),
            format!("{name} peers fell from {baseline} to {current}"),
        ));
    }

    alerts
}

#[derive(Clone)]
struct SentAlert {
    severity: Severity,
    title: String,
    last_sent: Instant,
}

pub struct NotifyEngine {
    sent: HashMap<String, SentAlert>,
}

impl NotifyEngine {
    pub fn new() -> Self {
        Self { sent: HashMap::new() }
    }

    /// Evaluate current conditions against what was already sent, returning
    /// exactly the alerts (new, escalated, renotified, or resolved) worth
    /// telling the user about right now.
    pub fn evaluate(
        &mut self,
        seconds_since_last_block: f64,
        peer_drops: &[PeerDrop],
        now: Instant,
    ) -> Vec<AlertEvent> {
        let active = build_active_alerts(seconds_since_last_block, peer_drops);
        let mut out = Vec::new();
        let mut next: HashMap<String, SentAlert> = HashMap::new();

        for (id, severity, title, body) in active {
            match self.sent.get(&id) {
                None => {
                    out.push(AlertEvent { id: id.clone(), severity, title: title.clone(), body, resolved: false });
                    next.insert(id, SentAlert { severity, title, last_sent: now });
                }
                Some(prev) if severity > prev.severity => {
                    out.push(AlertEvent { id: id.clone(), severity, title: title.clone(), body, resolved: false });
                    next.insert(id, SentAlert { severity, title, last_sent: now });
                }
                Some(prev) if severity == Severity::Critical && now.duration_since(prev.last_sent) >= RENOTIFY_COOLDOWN => {
                    out.push(AlertEvent { id: id.clone(), severity, title: title.clone(), body, resolved: false });
                    next.insert(id, SentAlert { severity, title, last_sent: now });
                }
                Some(prev) => {
                    // Still active, nothing new to say — carry the original
                    // last_sent forward so the renotify clock keeps ticking.
                    next.insert(id, SentAlert { severity, title, last_sent: prev.last_sent });
                }
            }
        }

        for (id, prev) in self.sent.iter() {
            if !next.contains_key(id) {
                out.push(AlertEvent {
                    id: id.clone(),
                    severity: prev.severity,
                    title: format!("Resolved: {}", prev.title),
                    body: "Back to normal.".to_string(),
                    resolved: true,
                });
            }
        }

        self.sent = next;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_then_stays_quiet_while_stable() {
        let mut engine = NotifyEngine::new();
        let t0 = Instant::now();
        let first = engine.evaluate(7.0, &[], t0);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].severity, Severity::Warning);
        assert!(!first[0].resolved);

        let second = engine.evaluate(7.5, &[], t0);
        assert!(second.is_empty(), "should not re-notify an unchanged warning");
    }

    #[test]
    fn escalates_and_resolves() {
        let mut engine = NotifyEngine::new();
        let t0 = Instant::now();
        engine.evaluate(7.0, &[], t0); // warning

        let escalated = engine.evaluate(25.0, &[], t0);
        assert_eq!(escalated.len(), 1);
        assert_eq!(escalated[0].severity, Severity::Critical);

        let resolved = engine.evaluate(1.0, &[], t0);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].resolved);
    }

    #[test]
    fn peer_drop_to_zero_is_critical() {
        let mut engine = NotifyEngine::new();
        let t0 = Instant::now();
        let drops = vec![(1u64, "my-validator".to_string(), 8u32, 0u32)];
        let alerts = engine.evaluate(0.0, &drops, t0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, Severity::Critical);
        assert!(alerts[0].id.starts_with("peer-drop-"));
    }
}
