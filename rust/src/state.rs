//! In-memory telemetry state: applies parsed feed events, and tracks just
//! enough history (per-node peer samples, time of last new block) to drive
//! the two alert conditions this app cares about.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::classify::{classify_node, NodeKind};
use crate::feed::TelemetryEvent;

#[derive(Debug, Clone, uniffi::Record)]
pub struct NodeInfo {
    pub id: u64,
    pub name: String,
    pub kind_label: String,
    pub is_validator: bool,
    pub peers: u32,
    pub best_block: u64,
}

/// A chain the feed carries, as announced by it — the source of the network
/// picker, so no genesis hash has to be hardcoded and go stale.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChainOption {
    pub genesis: String,
    pub label: String,
    pub node_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NetworkSummary {
    pub best_block: u64,
    pub finalized_block: u64,
    pub avg_block_time_ms: Option<u64>,
    pub seconds_since_last_block: f64,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct Snapshot {
    pub nodes: Vec<NodeInfo>,
    pub summary: Option<NetworkSummary>,
    pub chains: Vec<ChainOption>,
}

const PEER_HISTORY_WINDOW_SECS: u64 = 90;
/// Ignore drops for nodes with very few peers to begin with — noise, not signal.
const PEER_DROP_BASELINE_MIN: u32 = 4;

struct PeerHistory {
    samples: VecDeque<(Instant, u32)>,
}

impl PeerHistory {
    fn new() -> Self {
        Self { samples: VecDeque::new() }
    }

    fn push(&mut self, now: Instant, peers: u32) {
        self.samples.push_back((now, peers));
        while let Some(&(t, _)) = self.samples.front() {
            if now.duration_since(t).as_secs() > PEER_HISTORY_WINDOW_SECS {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Highest peer count seen in the window, excluding the very latest sample —
    /// i.e. "what this node's peers looked like just before now".
    fn baseline_max(&self) -> Option<u32> {
        let len = self.samples.len();
        if len < 2 {
            return None;
        }
        self.samples.iter().take(len - 1).map(|&(_, p)| p).max()
    }
}

struct NodeEntry {
    name: String,
    kind: NodeKind,
    peers: u32,
    best_block: u64,
    peer_history: PeerHistory,
}

/// A node whose peer count just dropped sharply: (id, name, previous baseline, current).
pub type PeerDrop = (u64, String, u32, u32);

pub struct TelemetryState {
    nodes: HashMap<u64, NodeEntry>,
    chains: HashMap<String, ChainOption>,
    best_block: u64,
    finalized_block: u64,
    avg_block_time_ms: Option<u64>,
    last_block_seen_at: Instant,
}

impl TelemetryState {
    pub fn new(now: Instant) -> Self {
        Self {
            nodes: HashMap::new(),
            chains: HashMap::new(),
            best_block: 0,
            finalized_block: 0,
            avg_block_time_ms: None,
            last_block_seen_at: now,
        }
    }

    pub fn apply(&mut self, event: TelemetryEvent, now: Instant) {
        match event {
            TelemetryEvent::AddedNode { id, name, peers, best_block } => {
                let kind = classify_node(&name);
                let mut entry =
                    NodeEntry { name, kind, peers, best_block, peer_history: PeerHistory::new() };
                entry.peer_history.push(now, peers);
                if best_block > self.best_block {
                    self.best_block = best_block;
                    self.last_block_seen_at = now;
                }
                self.nodes.insert(id, entry);
            }
            TelemetryEvent::RemovedNode { id } => {
                self.nodes.remove(&id);
            }
            TelemetryEvent::ImportedBlock { id, block_number } => {
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.best_block = block_number;
                }
                if block_number > self.best_block {
                    self.best_block = block_number;
                    self.last_block_seen_at = now;
                }
            }
            TelemetryEvent::FinalizedBlock { block_number } => {
                if block_number > self.finalized_block {
                    self.finalized_block = block_number;
                }
            }
            TelemetryEvent::NodeStats { id, peers } => {
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.peers = peers;
                    n.peer_history.push(now, peers);
                }
            }
            TelemetryEvent::BestBlock { block_number, avg_block_time_ms } => {
                self.avg_block_time_ms = avg_block_time_ms;
                if block_number > self.best_block {
                    self.best_block = block_number;
                    self.last_block_seen_at = now;
                }
            }
            TelemetryEvent::BestFinalized { block_number } => {
                if block_number > self.finalized_block {
                    self.finalized_block = block_number;
                }
            }
            TelemetryEvent::AddedChain { label, genesis, node_count } => {
                self.chains
                    .insert(genesis.clone(), ChainOption { genesis, label, node_count });
            }
        }
    }

    pub fn snapshot(&self, now: Instant) -> Snapshot {
        let mut nodes: Vec<NodeInfo> = self
            .nodes
            .iter()
            .map(|(&id, n)| NodeInfo {
                id,
                name: n.name.clone(),
                kind_label: n.kind.label().to_string(),
                is_validator: n.kind == NodeKind::Validator,
                peers: n.peers,
                best_block: n.best_block,
            })
            .collect();

        // Validators first, then worst-peers-first within each group, so
        // problem nodes surface at the top of a "minimal" single-screen list.
        nodes.sort_by(|a, b| {
            b.is_validator
                .cmp(&a.is_validator)
                .then(a.peers.cmp(&b.peers))
                .then(a.name.cmp(&b.name))
        });

        // Sorted by label so the picker keeps a stable order rather than
        // reshuffling as node counts move.
        let mut chains: Vec<ChainOption> = self.chains.values().cloned().collect();
        chains.sort_by(|a, b| a.label.cmp(&b.label));

        let summary = if self.best_block > 0 {
            Some(NetworkSummary {
                best_block: self.best_block,
                finalized_block: self.finalized_block,
                avg_block_time_ms: self.avg_block_time_ms,
                seconds_since_last_block: now.duration_since(self.last_block_seen_at).as_secs_f64(),
            })
        } else {
            None
        };

        Snapshot { nodes, summary, chains }
    }

    pub fn seconds_since_last_block(&self, now: Instant) -> f64 {
        now.duration_since(self.last_block_seen_at).as_secs_f64()
    }

    pub fn peer_drop_candidates(&self) -> Vec<PeerDrop> {
        self.nodes
            .iter()
            .filter_map(|(&id, n)| {
                let baseline = n.peer_history.baseline_max()?;
                let dropped_enough = baseline.saturating_sub(n.peers) >= 2;
                let halved = n.peers <= baseline / 2;
                if baseline >= PEER_DROP_BASELINE_MIN && halved && dropped_enough {
                    Some((id, n.name.clone(), baseline, n.peers))
                } else {
                    None
                }
            })
            .collect()
    }
}
