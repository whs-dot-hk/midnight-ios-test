//! The public Substrate-telemetry feed Midnight nodes report to.
//!
//! The set of chains carried by the feed is *not* hardcoded here — the feed
//! announces it on connect (see `TelemetryEvent::AddedChain`), which is both
//! self-correcting and where node counts come from. Only the initial
//! subscription target, used before that announcement arrives, is fixed.

pub const DEFAULT_FEED_URL: &str = "wss://telemetry.shielded.tools/feed/";

/// Midnight Mainnet's genesis hash — a public chain identifier broadcast in
/// every block header.
pub const DEFAULT_GENESIS: &str =
    "0x1941ca8e2bb88146c14dea084d3be7eb6e96ca7135429c543848b628124f2854";
