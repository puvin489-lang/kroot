mod common;

use analyzers::{AnalysisInput, GraphAnalyzer, NetworkReachabilityAnalyzer};
use graph::DependencyGraphBuilder;
use std::collections::BTreeMap;
use types::{
    AnalysisContext, AnalysisContextBuilder, IngressState, NamespaceState, NetworkPolicyPeerState,
    NetworkPolicyPortState, NetworkPolicyRuleState, NetworkPolicyState, PodPortState,
    PodSchedulingState, PodState, ServicePortState, ServiceState,
};

fn run_graph_analyzer(
    analyzer: &dyn GraphAnalyzer,
    ctx: &AnalysisContext,
) -> Option<types::Diagnosis> {
    let graph = DependencyGraphBuilder::from_context(ctx);
    analyzer.analyze_graph(&AnalysisInput {
        context: ctx,
        graph: &graph,
    })
}

#[test]
fn detects_ingress_path_blocked_by_namespace_selector_and_named_port() {
    let backend = PodState {
        ports: vec![PodPortState {
            name: Some("http".to_string()),
            protocol: "TCP".to_string(),
            container_port: 8080,
        }],
        ..common::base_pod()
    };
    let client = PodState {
        name: "frontend".to_string(),
        namespace: "web".to_string(),
        phase: "Running".to_string(),
        restart_count: 0,
        controller_kind: None,
        controller_name: None,
        node: "node-2".to_string(),
        pod_labels: BTreeMap::from([("app".to_string(), "frontend".to_string())]),
        scheduling: PodSchedulingState {
            unschedulable: false,
            reason: None,
            message: None,
        },
        service_selectors: vec![],
        container_states: vec![],
        dependencies: vec![],
        persistent_volume_claims: vec![],
        ports: vec![],
    };
    let ingress = IngressState {
        name: "payments-ing".to_string(),
        namespace: "prod".to_string(),
        backend_services: vec!["payments".to_string()],
    };
    let service = ServiceState {
        name: "payments".to_string(),
        namespace: "prod".to_string(),
        selector: BTreeMap::from([("app".to_string(), "payments-api".to_string())]),
        matched_pods: vec!["payments-api".to_string()],
        ports: vec![ServicePortState {
            name: Some("http".to_string()),
            protocol: "TCP".to_string(),
            port: 80,
            target_port: Some("http".to_string()),
        }],
    };
    let policy = NetworkPolicyState {
        name: "allow-only-http-alt-from-web-frontend".to_string(),
        namespace: "prod".to_string(),
        pod_selector: BTreeMap::from([("app".to_string(), "payments-api".to_string())]),
        pod_selector_expressions: vec![],
        policy_types: vec!["Ingress".to_string()],
        ingress_rule_count: 1,
        egress_rule_count: 0,
        ingress_peer_count: 1,
        egress_peer_count: 0,
        ingress_port_count: 1,
        egress_port_count: 0,
        default_deny_ingress: false,
        default_deny_egress: false,
        ingress_rules: vec![NetworkPolicyRuleState {
            peers: vec![NetworkPolicyPeerState {
                pod_selector: BTreeMap::from([("app".to_string(), "frontend".to_string())]),
                pod_selector_expressions: vec![],
                namespace_selector: BTreeMap::from([(
                    "kubernetes.io/metadata.name".to_string(),
                    "web".to_string(),
                )]),
                namespace_selector_expressions: vec![],
                has_pod_selector_expressions: false,
                has_namespace_selector_expressions: false,
                ip_block_cidr: None,
                ip_block_except: vec![],
            }],
            ports: vec![NetworkPolicyPortState {
                protocol: Some("TCP".to_string()),
                port: Some("http-alt".to_string()),
                end_port: None,
            }],
        }],
        egress_rules: vec![],
    };

    let ctx = AnalysisContextBuilder::new()
        .with_namespaces(vec![
            NamespaceState {
                name: "prod".to_string(),
                labels: BTreeMap::from([(
                    "kubernetes.io/metadata.name".to_string(),
                    "prod".to_string(),
                )]),
            },
            NamespaceState {
                name: "web".to_string(),
                labels: BTreeMap::from([(
                    "kubernetes.io/metadata.name".to_string(),
                    "web".to_string(),
                )]),
            },
        ])
        .with_pods(vec![backend, client])
        .with_ingresses(vec![ingress])
        .with_services(vec![service])
        .with_network_policies(vec![policy])
        .build();

    let diagnosis = run_graph_analyzer(&NetworkReachabilityAnalyzer, &ctx)
        .expect("expected reachability diagnosis");
    assert_eq!(
        diagnosis.message,
        "Network reachability blocked by NetworkPolicy"
    );
    assert!(
        diagnosis
            .evidence
            .iter()
            .any(|item| item.contains("Ingress/prod/payments-ing"))
    );
    assert!(
        diagnosis
            .evidence
            .iter()
            .any(|item| item.contains("blocked_ports=[80/TCP]"))
    );
}

#[test]
fn does_not_flag_egress_when_ipblock_allows_external() {
    let mut pod = common::base_pod();
    pod.name = "worker".to_string();
    pod.pod_labels = BTreeMap::from([("app".to_string(), "worker".to_string())]);

    let policy = NetworkPolicyState {
        name: "allow-external".to_string(),
        namespace: "prod".to_string(),
        pod_selector: BTreeMap::from([("app".to_string(), "worker".to_string())]),
        pod_selector_expressions: vec![],
        policy_types: vec!["Egress".to_string()],
        ingress_rule_count: 0,
        egress_rule_count: 1,
        ingress_peer_count: 0,
        egress_peer_count: 1,
        ingress_port_count: 0,
        egress_port_count: 0,
        default_deny_ingress: false,
        default_deny_egress: false,
        ingress_rules: vec![],
        egress_rules: vec![NetworkPolicyRuleState {
            peers: vec![NetworkPolicyPeerState {
                pod_selector: BTreeMap::new(),
                pod_selector_expressions: vec![],
                namespace_selector: BTreeMap::new(),
                namespace_selector_expressions: vec![],
                has_pod_selector_expressions: false,
                has_namespace_selector_expressions: false,
                ip_block_cidr: Some("0.0.0.0/0".to_string()),
                ip_block_except: vec![],
            }],
            ports: vec![],
        }],
    };
    let ctx = AnalysisContextBuilder::new()
        .with_pods(vec![pod])
        .with_network_policies(vec![policy])
        .build();

    let diagnosis = run_graph_analyzer(&NetworkReachabilityAnalyzer, &ctx);
    assert!(
        diagnosis.is_none(),
        "egress with external ipBlock allow should not be flagged"
    );
}
