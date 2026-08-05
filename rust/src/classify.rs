//! Best-effort node classification from its telemetry `--name`, purely for
//! display grouping (e.g. showing validators first in the list).

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NodeKind {
    Validator,
    FilterGateway,
    Boot,
    Bridge,
    SemiTrustedRpc,
    Rpc,
    Other,
}

impl NodeKind {
    pub fn label(&self) -> &'static str {
        match self {
            NodeKind::Validator => "Validator",
            NodeKind::FilterGateway => "Filter Gateway",
            NodeKind::Boot => "Boot Node",
            NodeKind::Bridge => "Bridge",
            NodeKind::SemiTrustedRpc => "Semi-Trusted RPC",
            NodeKind::Rpc => "RPC",
            NodeKind::Other => "Other",
        }
    }
}

/// Order matters — more specific substrings must be checked before general ones.
pub fn classify_node(name: &str) -> NodeKind {
    let lower = name.to_lowercase();
    if lower.is_empty() {
        return NodeKind::Other;
    }
    if lower.contains("validator") {
        return NodeKind::Validator;
    }
    if lower.contains("filter-gateway") || lower.contains("filter_gateway") {
        return NodeKind::FilterGateway;
    }
    if lower.contains("semi-trusted") || lower.contains("semi_trusted") {
        return NodeKind::SemiTrustedRpc;
    }
    if lower.contains("boot") {
        return NodeKind::Boot;
    }
    if lower.contains("bridge") {
        return NodeKind::Bridge;
    }
    if lower.contains("rpc") {
        return NodeKind::Rpc;
    }
    NodeKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_names() {
        assert_eq!(classify_node("sfi-validator-google"), NodeKind::Validator);
        assert_eq!(classify_node("mn-boot-1"), NodeKind::Boot);
        assert_eq!(classify_node("mn-rpc-2"), NodeKind::Rpc);
        assert_eq!(classify_node("semi-trusted-rpc-1"), NodeKind::SemiTrustedRpc);
        assert_eq!(classify_node("some-bridge"), NodeKind::Bridge);
        assert_eq!(classify_node(""), NodeKind::Other);
        assert_eq!(classify_node("whatever"), NodeKind::Other);
    }
}
