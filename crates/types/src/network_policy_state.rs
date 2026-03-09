use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyState {
    pub name: String,
    pub namespace: String,
    pub pod_selector: BTreeMap<String, String>,
    #[serde(default)]
    pub pod_selector_expressions: Vec<LabelSelectorRequirementState>,
    pub policy_types: Vec<String>,
    pub ingress_rule_count: usize,
    pub egress_rule_count: usize,
    pub ingress_peer_count: usize,
    pub egress_peer_count: usize,
    pub ingress_port_count: usize,
    pub egress_port_count: usize,
    pub default_deny_ingress: bool,
    pub default_deny_egress: bool,
    #[serde(default)]
    pub ingress_rules: Vec<NetworkPolicyRuleState>,
    #[serde(default)]
    pub egress_rules: Vec<NetworkPolicyRuleState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyRuleState {
    #[serde(default)]
    pub peers: Vec<NetworkPolicyPeerState>,
    #[serde(default)]
    pub ports: Vec<NetworkPolicyPortState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyPeerState {
    #[serde(default)]
    pub pod_selector: BTreeMap<String, String>,
    #[serde(default)]
    pub pod_selector_expressions: Vec<LabelSelectorRequirementState>,
    #[serde(default)]
    pub namespace_selector: BTreeMap<String, String>,
    #[serde(default)]
    pub namespace_selector_expressions: Vec<LabelSelectorRequirementState>,
    #[serde(default)]
    pub has_pod_selector_expressions: bool,
    #[serde(default)]
    pub has_namespace_selector_expressions: bool,
    pub ip_block_cidr: Option<String>,
    #[serde(default)]
    pub ip_block_except: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyPortState {
    pub protocol: Option<String>,
    pub port: Option<String>,
    pub end_port: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSelectorRequirementState {
    pub key: String,
    pub operator: String,
    #[serde(default)]
    pub values: Vec<String>,
}
