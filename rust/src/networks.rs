//! Known Midnight networks and the public Substrate-telemetry feed they report to.
//! Genesis hashes are public chain identifiers broadcast in every block header.

pub struct NetworkConfig {
    pub id: &'static str,
    pub genesis: &'static str,
}

pub const NETWORKS: &[NetworkConfig] = &[
    NetworkConfig {
        id: "mainnet",
        genesis: "0x1941ca8e2bb88146c14dea084d3be7eb6e96ca7135429c543848b628124f2854",
    },
    NetworkConfig {
        id: "preprod",
        genesis: "0xdf831b09a8baa92badf47762ce5ac439b7e47e3ed3d39600cfdd44fad552361b",
    },
    NetworkConfig {
        id: "preview",
        genesis: "0x801d3fc306115a3b538ea9498881c176376f8e3213464fe620fc1f359d13b880",
    },
];

pub const DEFAULT_NETWORK_ID: &str = "mainnet";
pub const DEFAULT_FEED_URL: &str = "wss://telemetry.shielded.tools/feed/";

pub fn genesis_for(network_id: &str) -> Option<&'static str> {
    NETWORKS.iter().find(|n| n.id == network_id).map(|n| n.genesis)
}
