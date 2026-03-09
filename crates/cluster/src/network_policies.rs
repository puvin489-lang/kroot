use std::future::Future;
use std::pin::Pin;

use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::networking::v1::NetworkPolicy;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, LabelSelectorRequirement};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{Api, Client, api::ListParams};
use types::{
    AnalysisContextBuilder, LabelSelectorRequirementState, NetworkPolicyPeerState,
    NetworkPolicyPortState, NetworkPolicyRuleState, NetworkPolicyState,
};

use crate::collector::{ClusterResult, CollectInput, CollectScope, Collector};
use crate::pods::fetch_target_pod;

pub struct NetworkPolicyCollector;

impl Collector for NetworkPolicyCollector {
    fn collect<'a>(
        &'a self,
        client: &'a Client,
        input: &'a CollectInput,
        builder: AnalysisContextBuilder,
    ) -> Pin<Box<dyn Future<Output = ClusterResult<AnalysisContextBuilder>> + 'a>> {
        Box::pin(async move {
            let policies = match &input.scope {
                CollectScope::Pod(pod_name) => {
                    let pod = fetch_target_pod(client, &input.namespace, pod_name).await?;
                    collect_network_policies_for_pod(client, &input.namespace, &pod).await?
                }
                CollectScope::Cluster => {
                    collect_namespace_network_policies(client, &input.namespace).await?
                }
            };
            Ok(builder.with_network_policies(policies))
        })
    }
}

async fn collect_namespace_network_policies(
    client: &Client,
    namespace: &str,
) -> ClusterResult<Vec<NetworkPolicyState>> {
    let policies_api: Api<NetworkPolicy> = Api::namespaced(client.clone(), namespace);
    let policies = policies_api.list(&ListParams::default()).await?;
    Ok(policies
        .items
        .into_iter()
        .filter_map(normalize_network_policy_state)
        .collect())
}

async fn collect_network_policies_for_pod(
    client: &Client,
    namespace: &str,
    pod: &Pod,
) -> ClusterResult<Vec<NetworkPolicyState>> {
    let pod_labels = pod.metadata.labels.clone().unwrap_or_default();
    let policies = collect_namespace_network_policies(client, namespace).await?;
    Ok(policies
        .into_iter()
        .filter(|policy| {
            selector_matches_labels(&policy.pod_selector, &pod_labels)
                && selector_requirements_match(&policy.pod_selector_expressions, &pod_labels)
        })
        .collect())
}

fn normalize_network_policy_state(policy: NetworkPolicy) -> Option<NetworkPolicyState> {
    let name = policy.metadata.name?;
    let namespace = policy
        .metadata
        .namespace
        .unwrap_or_else(|| "default".to_string());
    let spec = policy.spec?;

    let (pod_selector, pod_selector_expressions, _has_pod_selector_expressions) =
        normalize_label_selector(spec.pod_selector.as_ref());
    let ingress_rule_count = spec.ingress.as_ref().map_or(0, |rules| rules.len());
    let egress_rule_count = spec.egress.as_ref().map_or(0, |rules| rules.len());
    let ingress_peer_count = spec.ingress.as_ref().map_or(0, |rules| {
        rules
            .iter()
            .map(|rule| rule.from.as_ref().map_or(0, |from| from.len()))
            .sum()
    });
    let egress_peer_count = spec.egress.as_ref().map_or(0, |rules| {
        rules
            .iter()
            .map(|rule| rule.to.as_ref().map_or(0, |to| to.len()))
            .sum()
    });
    let ingress_port_count = spec.ingress.as_ref().map_or(0, |rules| {
        rules
            .iter()
            .map(|rule| rule.ports.as_ref().map_or(0, |ports| ports.len()))
            .sum()
    });
    let egress_port_count = spec.egress.as_ref().map_or(0, |rules| {
        rules
            .iter()
            .map(|rule| rule.ports.as_ref().map_or(0, |ports| ports.len()))
            .sum()
    });
    let policy_types = spec.policy_types.unwrap_or_else(|| {
        let mut types = vec!["Ingress".to_string()];
        if spec.egress.is_some() {
            types.push("Egress".to_string());
        }
        types
    });
    let default_deny_ingress =
        policy_types.iter().any(|t| t == "Ingress") && ingress_rule_count == 0;
    let default_deny_egress = policy_types.iter().any(|t| t == "Egress") && egress_rule_count == 0;
    let ingress_rules = spec
        .ingress
        .as_ref()
        .map(|rules| {
            rules
                .iter()
                .map(|rule| NetworkPolicyRuleState {
                    peers: rule
                        .from
                        .as_ref()
                        .map(|peers| {
                            peers
                                .iter()
                                .map(normalize_network_policy_peer)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    ports: rule
                        .ports
                        .as_ref()
                        .map(|ports| {
                            ports
                                .iter()
                                .map(normalize_network_policy_port)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let egress_rules = spec
        .egress
        .as_ref()
        .map(|rules| {
            rules
                .iter()
                .map(|rule| NetworkPolicyRuleState {
                    peers: rule
                        .to
                        .as_ref()
                        .map(|peers| {
                            peers
                                .iter()
                                .map(normalize_network_policy_peer)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    ports: rule
                        .ports
                        .as_ref()
                        .map(|ports| {
                            ports
                                .iter()
                                .map(normalize_network_policy_port)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(NetworkPolicyState {
        name,
        namespace,
        pod_selector,
        pod_selector_expressions,
        policy_types,
        ingress_rule_count,
        egress_rule_count,
        ingress_peer_count,
        egress_peer_count,
        ingress_port_count,
        egress_port_count,
        default_deny_ingress,
        default_deny_egress,
        ingress_rules,
        egress_rules,
    })
}

fn normalize_network_policy_peer(
    peer: &k8s_openapi::api::networking::v1::NetworkPolicyPeer,
) -> NetworkPolicyPeerState {
    let (pod_selector, pod_selector_expressions, has_pod_selector_expressions) =
        normalize_label_selector(peer.pod_selector.as_ref());
    let (namespace_selector, namespace_selector_expressions, has_namespace_selector_expressions) =
        normalize_label_selector(peer.namespace_selector.as_ref());

    NetworkPolicyPeerState {
        pod_selector,
        pod_selector_expressions,
        namespace_selector,
        namespace_selector_expressions,
        has_pod_selector_expressions,
        has_namespace_selector_expressions,
        ip_block_cidr: peer.ip_block.as_ref().map(|ip| ip.cidr.clone()),
        ip_block_except: peer
            .ip_block
            .as_ref()
            .and_then(|ip| ip.except.clone())
            .unwrap_or_default(),
    }
}

fn normalize_network_policy_port(
    port: &k8s_openapi::api::networking::v1::NetworkPolicyPort,
) -> NetworkPolicyPortState {
    NetworkPolicyPortState {
        protocol: port.protocol.clone(),
        port: port.port.as_ref().map(int_or_string_to_string),
        end_port: port.end_port,
    }
}

fn normalize_label_selector(
    selector: Option<&LabelSelector>,
) -> (
    std::collections::BTreeMap<String, String>,
    Vec<LabelSelectorRequirementState>,
    bool,
) {
    let Some(selector) = selector else {
        return (std::collections::BTreeMap::new(), Vec::new(), false);
    };
    let expressions = selector
        .match_expressions
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(normalize_label_selector_requirement)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (
        selector.match_labels.clone().unwrap_or_default(),
        expressions.clone(),
        !expressions.is_empty(),
    )
}

fn normalize_label_selector_requirement(
    requirement: &LabelSelectorRequirement,
) -> LabelSelectorRequirementState {
    LabelSelectorRequirementState {
        key: requirement.key.clone(),
        operator: requirement.operator.clone(),
        values: requirement.values.clone().unwrap_or_default(),
    }
}

fn int_or_string_to_string(value: &IntOrString) -> String {
    match value {
        IntOrString::Int(port) => port.to_string(),
        IntOrString::String(name) => name.clone(),
    }
}

fn selector_matches_labels(
    selector: &std::collections::BTreeMap<String, String>,
    labels: &std::collections::BTreeMap<String, String>,
) -> bool {
    selector
        .iter()
        .all(|(key, value)| labels.get(key) == Some(value))
}

fn selector_requirements_match(
    requirements: &[LabelSelectorRequirementState],
    labels: &std::collections::BTreeMap<String, String>,
) -> bool {
    requirements.iter().all(|requirement| {
        let value = labels.get(&requirement.key);
        match requirement.operator.as_str() {
            "In" => value.is_some_and(|current| requirement.values.contains(current)),
            "NotIn" => value.is_none_or(|current| !requirement.values.contains(current)),
            "Exists" => value.is_some(),
            "DoesNotExist" => value.is_none(),
            _ => false,
        }
    })
}

#[allow(dead_code)]
fn full_selector_matches_labels(
    selector: &LabelSelector,
    labels: &std::collections::BTreeMap<String, String>,
) -> bool {
    let matches_labels = selector.match_labels.as_ref().is_none_or(|match_labels| {
        match_labels
            .iter()
            .all(|(key, value)| labels.get(key) == Some(value))
    });
    let matches_expressions = selector
        .match_expressions
        .as_ref()
        .is_none_or(|exprs| exprs.iter().all(|expr| expression_matches(expr, labels)));
    matches_labels && matches_expressions
}

#[allow(dead_code)]
fn expression_matches(
    requirement: &LabelSelectorRequirement,
    labels: &std::collections::BTreeMap<String, String>,
) -> bool {
    let value = labels.get(&requirement.key);
    match requirement.operator.as_str() {
        "In" => value.is_some_and(|current| {
            requirement
                .values
                .as_ref()
                .is_some_and(|v| v.contains(current))
        }),
        "NotIn" => value.is_none_or(|current| {
            requirement
                .values
                .as_ref()
                .is_some_and(|v| !v.contains(current))
        }),
        "Exists" => value.is_some(),
        "DoesNotExist" => value.is_none(),
        _ => false,
    }
}
