//! Rust core for the Midnight Telemetry iOS app.
//!
//! All networking, protocol parsing, state, and alert-decision logic lives
//! here. Swift is left with exactly the things Rust cannot do on iOS: render
//! UI and call `UNUserNotificationCenter`.
//!
//! Data source: the public Substrate-telemetry feed at
//! `wss://telemetry.shielded.tools/feed/` (same wire protocol as
//! telemetry.polkadot.io / paritytech/substrate-telemetry).

uniffi::setup_scaffolding!();

mod classify;
mod client;
mod feed;
mod health;
mod networks;
mod state;

use std::sync::Arc;
use std::sync::Mutex;

pub use health::{AlertEvent, Severity};
pub use state::{NetworkSummary, NodeInfo, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionStatus {
    Connecting,
    Live,
    Reconnecting,
}

/// Implemented in Swift, called from Rust's background telemetry thread.
/// Swift implementations must hop to the main thread before touching UI state.
#[uniffi::export(with_foreign)]
pub trait TelemetryDelegate: Send + Sync {
    fn on_snapshot(&self, snapshot: Snapshot);
    fn on_alert(&self, alert: AlertEvent);
    fn on_status_changed(&self, status: ConnectionStatus);
}

#[uniffi::export]
pub fn default_feed_url() -> String {
    networks::DEFAULT_FEED_URL.to_string()
}

#[uniffi::export]
pub fn default_network_id() -> String {
    networks::DEFAULT_NETWORK_ID.to_string()
}

#[uniffi::export]
pub fn default_block_stall_secs() -> f64 {
    health::DEFAULT_BLOCK_STALL_SECS
}

#[derive(uniffi::Object)]
pub struct TelemetryClient {
    feed_url: String,
    genesis: String,
    block_stall_secs: f64,
    handle: Mutex<Option<client::ClientHandle>>,
}

#[uniffi::export]
impl TelemetryClient {
    /// `feed_url` — pass `default_feed_url()` unless you have your own endpoint.
    /// `network_id` — "mainnet", "preprod", or "preview" (see networks::NETWORKS);
    /// falls back to mainnet if unrecognized.
    /// `block_stall_secs` — how long without a new block before alerting; pass
    /// `default_block_stall_secs()` unless the user has set their own.
    #[uniffi::constructor]
    pub fn new(feed_url: String, network_id: String, block_stall_secs: f64) -> Arc<Self> {
        let genesis = networks::genesis_for(&network_id)
            .or_else(|| networks::genesis_for(networks::DEFAULT_NETWORK_ID))
            .expect("default network must have a genesis hash")
            .to_string();
        Arc::new(Self { feed_url, genesis, block_stall_secs, handle: Mutex::new(None) })
    }

    /// Starts (or is a no-op if already running) the background telemetry
    /// thread. `delegate` receives snapshots and alerts until `stop()` is called.
    pub fn start(&self, delegate: Arc<dyn TelemetryDelegate>) {
        let mut guard = self.handle.lock().expect("telemetry handle lock poisoned");
        if guard.is_some() {
            return;
        }
        *guard = Some(client::ClientHandle::start(
            self.feed_url.clone(),
            self.genesis.clone(),
            self.block_stall_secs,
            delegate,
        ));
    }

    pub fn stop(&self) {
        let mut guard = self.handle.lock().expect("telemetry handle lock poisoned");
        if let Some(mut h) = guard.take() {
            h.stop();
        }
    }
}
