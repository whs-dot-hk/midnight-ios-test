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

/// Default for the user-configurable block-stall threshold. Midnight's actual
/// block time averages ~6s, so this needs real headroom above that average —
/// otherwise normal per-block jitter crosses a too-tight threshold on its own
/// and every other block fires a false alarm.
pub const DEFAULT_BLOCK_STALL_SECS: f64 = 15.0;
const RENOTIFY_COOLDOWN: Duration = Duration::from_secs(10 * 60);

fn build_active_alerts(
    seconds_since_last_block: f64,
    peer_drops: &[PeerDrop],
    block_stall_secs: f64,
) -> Vec<(String, Severity, String, String)> {
    let mut alerts = Vec::new();

    if seconds_since_last_block > block_stall_secs {
        alerts.push((
            "block-stall".to_string(),
            Severity::Critical,
            "Block production stalled".to_string(),
            format!(
                "No new block in {:.0}s (threshold {:.0}s)",
                seconds_since_last_block, block_stall_secs
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
    block_stall_secs: f64,
    sent: HashMap<String, SentAlert>,
}

impl NotifyEngine {
    pub fn new(block_stall_secs: f64) -> Self {
        Self { block_stall_secs, sent: HashMap::new() }
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
        let active = build_active_alerts(seconds_since_last_block, peer_drops, self.block_stall_secs);
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
        let mut engine = NotifyEngine::new(15.0);
        let t0 = Instant::now();
        let first = engine.evaluate(16.0, &[], t0);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].severity, Severity::Critical);
        assert!(!first[0].resolved);

        let second = engine.evaluate(16.5, &[], t0);
        assert!(second.is_empty(), "should not re-notify an unchanged stall");
    }

    #[test]
    fn fires_then_resolves() {
        let mut engine = NotifyEngine::new(15.0);
        let t0 = Instant::now();
        engine.evaluate(16.0, &[], t0);

        let resolved = engine.evaluate(1.0, &[], t0);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].resolved);
    }

    #[test]
    fn peer_drop_to_zero_is_critical() {
        let mut engine = NotifyEngine::new(15.0);
        let t0 = Instant::now();
        let drops = vec![(1u64, "my-validator".to_string(), 8u32, 0u32)];
        let alerts = engine.evaluate(0.0, &drops, t0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, Severity::Critical);
        assert!(alerts[0].id.starts_with("peer-drop-"));
    }
}
