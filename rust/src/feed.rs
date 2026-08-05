//! Parser for the public Substrate-telemetry WebSocket feed
//! (`wss://telemetry.shielded.tools/feed/`, same wire format as
//! `paritytech/substrate-telemetry` and telemetry.polkadot.io).
//!
//! Wire format: a flat JSON array of alternating `[action, payload, action, payload, ...]`
//! pairs. Each payload is itself a nested array. Unknown/irrelevant action codes are
//! skipped — this app only tracks the fields it needs (block height/hash, peers).

use serde_json::Value;

mod action {
    pub const BEST_BLOCK: i64 = 1;
    pub const BEST_FINALIZED: i64 = 2;
    pub const ADDED_NODE: i64 = 3;
    pub const REMOVED_NODE: i64 = 4;
    pub const IMPORTED_BLOCK: i64 = 6;
    pub const FINALIZED_BLOCK: i64 = 7;
    pub const NODE_STATS: i64 = 8;
    pub const ADDED_CHAIN: i64 = 11;
}

#[derive(Debug, Clone, PartialEq)]
pub enum TelemetryEvent {
    AddedNode {
        id: u64,
        name: String,
        peers: u32,
        best_block: u64,
    },
    RemovedNode {
        id: u64,
    },
    ImportedBlock {
        id: u64,
        block_number: u64,
    },
    FinalizedBlock {
        block_number: u64,
    },
    NodeStats {
        id: u64,
        peers: u32,
    },
    BestBlock {
        block_number: u64,
        avg_block_time_ms: Option<u64>,
    },
    BestFinalized {
        block_number: u64,
    },
    /// The feed announces every chain it carries on connect, regardless of
    /// which one we subscribe to — this is where the network list comes from.
    AddedChain {
        label: String,
        genesis: String,
        node_count: u32,
    },
}

fn arr(v: &Value) -> Option<&Vec<Value>> {
    v.as_array()
}
fn u64_at(a: &[Value], i: usize) -> Option<u64> {
    a.get(i).and_then(Value::as_u64)
}
fn str_at(a: &[Value], i: usize) -> String {
    a.get(i).and_then(Value::as_str).unwrap_or("").to_string()
}

pub fn parse_feed_message(raw: &str) -> Vec<TelemetryEvent> {
    let batch: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items = match batch.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    if items.len() < 2 || items.len() % 2 != 0 {
        return Vec::new();
    }

    let mut events = Vec::new();
    let mut i = 0;
    while i + 1 < items.len() {
        let action_code = items[i].as_i64().unwrap_or(-1);
        let payload = match arr(&items[i + 1]) {
            Some(p) => p.as_slice(),
            None => {
                i += 2;
                continue;
            }
        };

        match action_code {
            action::ADDED_NODE => {
                // payload: [id, nodeDetails, nodeStats, nodeIO, nodeHw, blockDetails, location, startupTime]
                if let Some(id) = u64_at(payload, 0) {
                    let details = payload.get(1).and_then(arr);
                    let stats = payload.get(2).and_then(arr);
                    let block = payload.get(5).and_then(arr);
                    let name = details
                        .and_then(|d| d.first())
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let peers = stats
                        .and_then(|s| u64_at(s, 0))
                        .unwrap_or(0) as u32;
                    let best_block = block.and_then(|b| u64_at(b, 0)).unwrap_or(0);
                    events.push(TelemetryEvent::AddedNode {
                        id,
                        name,
                        peers,
                        best_block,
                    });
                }
            }
            action::REMOVED_NODE => {
                if let Some(id) = u64_at(payload, 0) {
                    events.push(TelemetryEvent::RemovedNode { id });
                }
            }
            action::IMPORTED_BLOCK => {
                // payload: [id, blockDetails]; blockDetails: [number, hash, ms, timestamp, propTime]
                if let (Some(id), Some(block)) = (u64_at(payload, 0), payload.get(1).and_then(arr)) {
                    events.push(TelemetryEvent::ImportedBlock {
                        id,
                        block_number: u64_at(block, 0).unwrap_or(0),
                    });
                }
            }
            action::FINALIZED_BLOCK => {
                // payload: [id, blockNumber, blockHash]
                events.push(TelemetryEvent::FinalizedBlock {
                    block_number: u64_at(payload, 1).unwrap_or(0),
                });
            }
            action::NODE_STATS => {
                // payload: [id, [peers, txCount]]
                if let (Some(id), Some(stats)) = (u64_at(payload, 0), payload.get(1).and_then(arr)) {
                    events.push(TelemetryEvent::NodeStats {
                        id,
                        peers: u64_at(stats, 0).unwrap_or(0) as u32,
                    });
                }
            }
            action::BEST_BLOCK => {
                // payload: [blockNumber, timestamp, maybeAvgMs]
                events.push(TelemetryEvent::BestBlock {
                    block_number: u64_at(payload, 0).unwrap_or(0),
                    avg_block_time_ms: u64_at(payload, 2),
                });
            }
            action::BEST_FINALIZED => {
                // payload: [blockNumber, blockHash]
                events.push(TelemetryEvent::BestFinalized {
                    block_number: u64_at(payload, 0).unwrap_or(0),
                });
            }
            action::ADDED_CHAIN => {
                // payload: [label, genesisHash, nodeCount]
                let genesis = str_at(payload, 1);
                if !genesis.is_empty() {
                    events.push(TelemetryEvent::AddedChain {
                        label: str_at(payload, 0),
                        genesis,
                        node_count: u64_at(payload, 2).unwrap_or(0) as u32,
                    });
                }
            }
            _ => {}
        }

        i += 2;
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_malformed_input() {
        assert!(parse_feed_message("not json").is_empty());
        assert!(parse_feed_message("[1]").is_empty()); // odd length
        assert!(parse_feed_message("{}").is_empty()); // not an array
    }

    #[test]
    fn parses_added_node() {
        let msg = r#"[3, [42, ["my-validator", "midnight-node", "1.0.0"], [7, 100], [], [], [12345, "0xabc", 0, 0, 0], null, 999]]"#;
        let events = parse_feed_message(msg);
        assert_eq!(
            events,
            vec![TelemetryEvent::AddedNode {
                id: 42,
                name: "my-validator".into(),
                peers: 7,
                best_block: 12345,
            }]
        );
    }

    #[test]
    fn parses_imported_block_and_best_block_batch() {
        let msg = r#"[6, [42, [12346, "0xdef", 50, 1710000000, 20]], 1, [12346, 1710000000, 6200]]"#;
        let events = parse_feed_message(msg);
        assert_eq!(
            events,
            vec![
                TelemetryEvent::ImportedBlock { id: 42, block_number: 12346 },
                TelemetryEvent::BestBlock { block_number: 12346, avg_block_time_ms: Some(6200) },
            ]
        );
    }

    #[test]
    fn parses_node_stats_and_removed_node() {
        let msg = r#"[8, [42, [3, 0]], 4, [42]]"#;
        let events = parse_feed_message(msg);
        assert_eq!(
            events,
            vec![
                TelemetryEvent::NodeStats { id: 42, peers: 3 },
                TelemetryEvent::RemovedNode { id: 42 },
            ]
        );
    }

    /// Payload shape captured from the live feed at telemetry.shielded.tools.
    #[test]
    fn parses_added_chain() {
        let msg = r#"[11, ["Midnight Mainnet", "0x1941ca8e", 37], 11, ["", "", 0]]"#;
        let events = parse_feed_message(msg);
        assert_eq!(
            events,
            vec![TelemetryEvent::AddedChain {
                label: "Midnight Mainnet".into(),
                genesis: "0x1941ca8e".into(),
                node_count: 37,
            }],
            "a chain with no genesis hash is unusable and must be skipped"
        );
    }

    #[test]
    fn skips_unknown_action_codes() {
        let msg = r#"[23, ["some", "telemetry", "info"], 1, [10, 1710000000, null]]"#;
        let events = parse_feed_message(msg);
        assert_eq!(
            events,
            vec![TelemetryEvent::BestBlock { block_number: 10, avg_block_time_ms: None }]
        );
    }
}
