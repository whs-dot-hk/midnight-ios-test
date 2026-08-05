//! Whether a node is a validator, inferred from its telemetry `--name`. The
//! app tracks validators only, so this is the filter applied to everything the
//! feed reports.

/// Midnight validators carry a `validator` segment in their name. Verified
/// against the live mainnet feed: all 13 validators match, and none of the 24
/// RPC / boot / bridge / filter-gateway / semi-trusted nodes do.
pub fn is_validator(name: &str) -> bool {
    name.to_lowercase().contains("validator")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names taken verbatim from the live mainnet feed.
    #[test]
    fn matches_validators_only() {
        for name in [
            "sfi-validator-google",
            "aton-validator",
            "bkd-validator-bullish",
            "stl-validator-whippet-humpback",
            "mnf-validator-1",
        ] {
            assert!(is_validator(name), "{name} should be a validator");
        }

        for name in [
            "stl-semi-trusted-rpc-glider-anchovy",
            "stl-boot-glider-spaniel",
            "stl-bridge-dog-weasel",
            "stl-filter-gateway-whippet-louse",
            "stl-rpc-llama-thrush",
            "bgo-standby",
            "",
        ] {
            assert!(!is_validator(name), "{name} should not be a validator");
        }
    }
}
