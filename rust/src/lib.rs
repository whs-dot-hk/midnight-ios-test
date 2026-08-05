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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub use health::{AlertEvent, Severity};
pub use state::{ChainOption, NetworkSummary, NodeInfo, Snapshot};

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
pub fn default_genesis() -> String {
    networks::DEFAULT_GENESIS.to_string()
}

#[uniffi::export]
pub fn default_block_stall_secs() -> f64 {
    health::DEFAULT_BLOCK_STALL_SECS
}

#[derive(uniffi::Object)]
pub struct TelemetryClient {
    feed_url: String,
    genesis: String,
    /// Shared with the running telemetry thread as f64 bits so the user can
    /// change the threshold without a reconnect.
    block_stall_secs: Arc<AtomicU64>,
    /// Set to stop the thread. Shared rather than owned, so no handle to the
    /// thread needs keeping — and therefore no lock to guard it.
    stop: Arc<AtomicBool>,
    started: AtomicBool,
}

#[uniffi::export]
impl TelemetryClient {
    /// `feed_url` — pass `default_feed_url()` unless you have your own endpoint.
    /// `genesis` — the chain to subscribe to, taken from `Snapshot.chains` (or
    /// `default_genesis()` before the feed has announced its chain list).
    /// `block_stall_secs` — how long without a new block before alerting; pass
    /// `default_block_stall_secs()` unless the user has set their own.
    #[uniffi::constructor]
    pub fn new(feed_url: String, genesis: String, block_stall_secs: f64) -> Arc<Self> {
        Arc::new(Self {
            feed_url,
            genesis,
            block_stall_secs: Arc::new(AtomicU64::new(block_stall_secs.to_bits())),
            stop: Arc::new(AtomicBool::new(false)),
            started: AtomicBool::new(false),
        })
    }

    /// Changes the stall threshold on a running client, taking effect on the
    /// next evaluation, without disturbing the connection.
    pub fn set_block_stall_secs(&self, secs: f64) {
        self.block_stall_secs.store(secs.to_bits(), Ordering::Relaxed);
    }

    /// Starts the background telemetry thread; `delegate` receives snapshots
    /// and alerts until `stop()` is called. A second call is a no-op, which
    /// matters because starting twice would open two subscriptions delivering
    /// duplicate callbacks.
    ///
    /// A client is single-use: once stopped it stays stopped, since the app
    /// constructs a new one per subscription.
    pub fn start(&self, delegate: Arc<dyn TelemetryDelegate>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        client::spawn(
            self.feed_url.clone(),
            self.genesis.clone(),
            self.block_stall_secs.clone(),
            self.stop.clone(),
            delegate,
        );
    }

    /// Signals the telemetry thread to finish and returns immediately, so this
    /// is safe to call from the UI thread. The thread exits on its next tick,
    /// which means a few delegate callbacks may still arrive afterwards —
    /// callers should ignore callbacks for a subscription they have dropped.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Releasing a client stops its thread; nothing else holds it, so this keeps a
/// forgotten client from leaving a connection running for the process lifetime.
impl Drop for TelemetryClient {
    fn drop(&mut self) {
        self.stop();
    }
}
