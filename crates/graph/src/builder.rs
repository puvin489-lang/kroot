use std::collections::{BTreeMap, BTreeSet};

use types::{
    AnalysisContext, DependencyStatus, LabelSelectorRequirementState, NetworkPolicyPeerState,
    NetworkPolicyPortState, NetworkPolicyState, PodDependencyKind, PodState, ServicePortState,
    ServiceState,
};

use crate::model::{DependencyGraph, Relation, ResourceId};

pub struct DependencyGraphBuilder;

impl DependencyGraphBuilder {
    pub fn from_context(ctx: &AnalysisContext) -> DependencyGraph {
        let mut graph = DependencyGraph::new();
        let pvc_statuses = pvc_status_by_name(ctx);
        let pv_exists_by_name = ctx
            .persistent_volumes
            .iter()
            .map(|pv| (pv.name.clone(), pv.exists))
            .collect::<BTreeMap<_, _>>();
        let storage_class_exists_by_name = ctx
            .storage_classes
            .iter()
            .map(|class| (class.name.clone(), class.exists))
            .collect::<BTreeMap<_, _>>();

        for pod in &ctx.pods {
            let pod_id = ResourceId::pod(&pod.namespace, &pod.name);
            graph.add_resource(pod_id.clone());

            if pod.controller_kind.as_deref() == Some("ReplicaSet") {
                if let Some(controller_name) = &pod.controller_name {
                    graph.add_relation_with_meta(
                        ResourceId::replica_set(&pod.namespace, controller_name),
                        pod_id.clone(),
                        Relation::OwnsPod,
                        Some(DependencyStatus::Present),
                        Some("metadata.ownerReferences".to_string()),
                        None,
                    );
                }
            }

            if pod.node != "unassigned" {
                graph.add_relation_with_meta(
                    pod_id.clone(),
                    ResourceId::node(&pod.node),
                    Relation::ScheduledOnNode,
                    Some(DependencyStatus::Present),
                    Some("spec.nodeName".to_string()),
                    None,
                );
            }

            for dependency in &pod.dependencies {
                match dependency.kind {
                    PodDependencyKind::Secret => {
                        graph.add_relation_with_meta(
                            pod_id.clone(),
                            ResourceId::secret(&pod.namespace, &dependency.name),
                            Relation::UsesSecret,
                            Some(dependency.status.clone()),
                            Some("pod.dependencies".to_string()),
                            None,
                        );
                    }
                    PodDependencyKind::ConfigMap => {
                        graph.add_relation_with_meta(
                            pod_id.clone(),
                            ResourceId::config_map(&pod.namespace, &dependency.name),
                            Relation::UsesConfigMap,
                            Some(dependency.status.clone()),
                            Some("pod.dependencies".to_string()),
                            None,
                        );
                    }
                    PodDependencyKind::Node => {}
                    PodDependencyKind::ServiceAccount => {}
                }
            }

            for claim_name in &pod.persistent_volume_claims {
                let (status, detail) = pvc_statuses
                    .get(&(pod.namespace.clone(), claim_name.clone()))
                    .cloned()
                    .unwrap_or((
                        DependencyStatus::Unknown,
                        "PVC state unavailable".to_string(),
                    ));
                graph.add_relation_with_meta(
                    pod_id.clone(),
                    ResourceId::persistent_volume_claim(&pod.namespace, claim_name),
                    Relation::MountsPersistentVolumeClaim,
                    Some(status),
                    Some("spec.volumes[].persistentVolumeClaim.claimName".to_string()),
                    Some(detail),
                );
            }
        }

        for deployment in &ctx.deployments {
            graph.add_resource(ResourceId::deployment(
                &deployment.namespace,
                &deployment.name,
            ));
        }

        for replica_set in &ctx.replica_sets {
            let replica_set_id = ResourceId::replica_set(&replica_set.namespace, &replica_set.name);
            graph.add_resource(replica_set_id.clone());
            if let Some(owner_deployment) = &replica_set.owner_deployment {
                graph.add_relation_with_meta(
                    ResourceId::deployment(&replica_set.namespace, owner_deployment),
                    replica_set_id,
                    Relation::OwnsReplicaSet,
                    Some(DependencyStatus::Present),
                    Some("metadata.ownerReferences".to_string()),
                    None,
                );
            }
        }

        for service in &ctx.services {
            let service_id = ResourceId::service(&service.namespace, &service.name);
            graph.add_resource(service_id.clone());
            let selector = service
                .selector
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");

            for pod_name in &service.matched_pods {
                graph.add_relation_with_meta(
                    service_id.clone(),
                    ResourceId::pod(&service.namespace, pod_name),
                    Relation::RoutesToPod,
                    None,
                    Some("spec.selector".to_string()),
                    Some(format!("selector=[{selector}]")),
                );
            }
        }

        for ingress in &ctx.ingresses {
            let ingress_id = ResourceId::ingress(&ingress.namespace, &ingress.name);
            graph.add_resource(ingress_id.clone());
            for service_name in &ingress.backend_services {
                graph.add_relation_with_meta(
                    ingress_id.clone(),
                    ResourceId::service(&ingress.namespace, service_name),
                    Relation::RoutesToService,
                    Some(DependencyStatus::Present),
                    Some("spec.defaultBackend/spec.rules[].http.paths[].backend".to_string()),
                    None,
                );
            }
        }

        for pvc in &ctx.persistent_volume_claims {
            if let Some(volume_name) = &pvc.volume_name {
                let status = if !pvc.exists || pv_exists_by_name.get(volume_name) == Some(&false) {
                    DependencyStatus::Missing
                } else {
                    DependencyStatus::Present
                };
                graph.add_relation_with_meta(
                    ResourceId::persistent_volume_claim(&pvc.namespace, &pvc.name),
                    ResourceId::persistent_volume(volume_name),
                    Relation::BindsPersistentVolume,
                    Some(status),
                    Some("spec.volumeName".to_string()),
                    Some(format!(
                        "PVC phase={} pv_exists={}",
                        pvc.phase,
                        pv_exists_by_name.get(volume_name).copied().unwrap_or(false)
                    )),
                );
            }

            if let Some(storage_class_name) = &pvc.storage_class_name {
                let status = if !pvc.exists
                    || storage_class_exists_by_name.get(storage_class_name) == Some(&false)
                {
                    DependencyStatus::Missing
                } else {
                    DependencyStatus::Present
                };
                graph.add_relation_with_meta(
                    ResourceId::persistent_volume_claim(&pvc.namespace, &pvc.name),
                    ResourceId::storage_class(storage_class_name),
                    Relation::UsesStorageClass,
                    Some(status),
                    Some("spec.storageClassName".to_string()),
                    Some(format!(
                        "PVC phase={} storage_class_exists={}",
                        pvc.phase,
                        storage_class_exists_by_name
                            .get(storage_class_name)
                            .copied()
                            .unwrap_or(false)
                    )),
                );
            }
        }

        for policy in &ctx.network_policies {
            let policy_id = ResourceId::network_policy(&policy.namespace, &policy.name);
            graph.add_resource(policy_id.clone());

            let applies_to_all =
                policy.pod_selector.is_empty() && policy.pod_selector_expressions.is_empty();
            for pod in ctx.pods.iter().filter(|pod| {
                pod.namespace == policy.namespace
                    && (applies_to_all
                        || selector_matches(
                            &policy.pod_selector,
                            &policy.pod_selector_expressions,
                            &pod.pod_labels,
                        ))
            }) {
                let detail = format!(
                    "types={:?} ingress_rules={} egress_rules={} ingress_peers={} egress_peers={} ingress_ports={} egress_ports={} default_deny_ingress={} default_deny_egress={}",
                    policy.policy_types,
                    policy.ingress_rule_count,
                    policy.egress_rule_count,
                    policy.ingress_peer_count,
                    policy.egress_peer_count,
                    policy.ingress_port_count,
                    policy.egress_port_count,
                    policy.default_deny_ingress,
                    policy.default_deny_egress
                );
                graph.add_relation_with_meta(
                    policy_id.clone(),
                    ResourceId::pod(&pod.namespace, &pod.name),
                    Relation::AppliesToPod,
                    Some(DependencyStatus::Present),
                    Some("spec.podSelector".to_string()),
                    Some(detail),
                );
            }
        }

        add_network_policy_block_edges(&mut graph, ctx);

        graph
    }
}

fn add_network_policy_block_edges(graph: &mut DependencyGraph, ctx: &AnalysisContext) {
    let mut policies_by_pod = BTreeMap::<(String, String), Vec<&NetworkPolicyState>>::new();
    let namespaces_by_name = namespace_labels_by_name(ctx);

    for policy in &ctx.network_policies {
        for pod in ctx
            .pods
            .iter()
            .filter(|pod| policy_selects_pod(policy, pod))
        {
            policies_by_pod
                .entry((pod.namespace.clone(), pod.name.clone()))
                .or_default()
                .push(policy);
        }
    }

    for ingress in &ctx.ingresses {
        for service_name in &ingress.backend_services {
            let Some(service) = ctx
                .services
                .iter()
                .find(|svc| svc.namespace == ingress.namespace && svc.name == *service_name)
            else {
                continue;
            };
            if let Some((policy, detail)) = ingress_blocking_policy_for_service(
                service,
                &ctx.pods,
                &policies_by_pod,
                &namespaces_by_name,
            ) {
                graph.add_relation_with_meta(
                    ResourceId::ingress(&ingress.namespace, &ingress.name),
                    ResourceId::network_policy(&policy.namespace, &policy.name),
                    Relation::BlockedByNetworkPolicy,
                    Some(DependencyStatus::Missing),
                    Some("networkpolicy.ingress.external".to_string()),
                    Some(detail),
                );
            }
        }
    }

    for service in &ctx.services {
        if let Some((policy, detail)) = service_blocking_policy_from_internal_clients(
            service,
            &ctx.pods,
            &policies_by_pod,
            &namespaces_by_name,
        ) {
            graph.add_relation_with_meta(
                ResourceId::service(&service.namespace, &service.name),
                ResourceId::network_policy(&policy.namespace, &policy.name),
                Relation::BlockedByNetworkPolicy,
                Some(DependencyStatus::Missing),
                Some("networkpolicy.ingress.internal".to_string()),
                Some(detail),
            );
        }
    }

    for pod in &ctx.pods {
        let Some(applied_policies) =
            policies_by_pod.get(&(pod.namespace.clone(), pod.name.clone()))
        else {
            continue;
        };

        let egress_policies = applied_policies
            .iter()
            .copied()
            .filter(|policy| policy_has_type(policy, "Egress"))
            .collect::<Vec<_>>();
        if egress_policies.is_empty() {
            continue;
        }

        if egress_policies_allow_any_destination(
            &egress_policies,
            pod,
            &ctx.pods,
            &namespaces_by_name,
        ) {
            continue;
        }

        let primary_policy = select_primary_policy(&egress_policies, "egress");
        let policy_names = egress_policies
            .iter()
            .map(|policy| format!("NetworkPolicy/{}/{}", policy.namespace, policy.name))
            .collect::<Vec<_>>();

        graph.add_relation_with_meta(
            ResourceId::pod(&pod.namespace, &pod.name),
            ResourceId::network_policy(&primary_policy.namespace, &primary_policy.name),
            Relation::BlockedByNetworkPolicy,
            Some(DependencyStatus::Missing),
            Some("networkpolicy.egress".to_string()),
            Some(format!(
                "egress has no matching peers/ports in context policies=[{}]",
                policy_names.join(","),
            )),
        );
    }
}

fn ingress_blocking_policy_for_service<'a>(
    service: &ServiceState,
    all_pods: &'a [PodState],
    policies_by_pod: &BTreeMap<(String, String), Vec<&'a NetworkPolicyState>>,
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> Option<(&'a NetworkPolicyState, String)> {
    let backend_pods = service_backend_pods(service, all_pods);
    if backend_pods.is_empty() {
        return None;
    }
    let service_ports = normalize_service_ports(service);
    if service_ports.is_empty() {
        return None;
    }

    for backend in &backend_pods {
        let ingress_policies = policies_by_pod
            .get(&(backend.namespace.clone(), backend.name.clone()))
            .map(|policies| {
                policies
                    .iter()
                    .copied()
                    .filter(|policy| policy_has_type(policy, "Ingress"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if ingress_policies.is_empty() {
            return None;
        }

        let blocked_ports = service_ports
            .iter()
            .filter(|port| {
                let resolved_ports = resolve_service_target_ports(port, backend);
                !ingress_allows_external(
                    &ingress_policies,
                    backend,
                    &resolved_ports,
                    namespaces_by_name,
                )
            })
            .map(service_port_label)
            .collect::<Vec<_>>();
        if blocked_ports.len() != service_ports.len() {
            return None;
        }

        let primary_policy = select_primary_policy(&ingress_policies, "ingress");
        let detail = format!(
            "path=Ingress -> Service/{}/{} -> Pod/{}/{} blocked_ports=[{}] reason=external peer/port not allowed",
            service.namespace,
            service.name,
            backend.namespace,
            backend.name,
            blocked_ports.join(","),
        );
        return Some((primary_policy, detail));
    }

    None
}

fn service_blocking_policy_from_internal_clients<'a>(
    service: &ServiceState,
    all_pods: &'a [PodState],
    policies_by_pod: &BTreeMap<(String, String), Vec<&'a NetworkPolicyState>>,
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> Option<(&'a NetworkPolicyState, String)> {
    let backend_pods = service_backend_pods(service, all_pods);
    if backend_pods.is_empty() {
        return None;
    }
    let service_ports = normalize_service_ports(service);
    if service_ports.is_empty() {
        return None;
    }

    let backend_ids = backend_pods
        .iter()
        .map(|pod| (pod.namespace.clone(), pod.name.clone()))
        .collect::<BTreeSet<_>>();
    let client_pods = all_pods
        .iter()
        .filter(|pod| !backend_ids.contains(&(pod.namespace.clone(), pod.name.clone())))
        .collect::<Vec<_>>();
    if client_pods.is_empty() {
        return None;
    }

    for backend in &backend_pods {
        let ingress_policies = policies_by_pod
            .get(&(backend.namespace.clone(), backend.name.clone()))
            .map(|policies| {
                policies
                    .iter()
                    .copied()
                    .filter(|policy| policy_has_type(policy, "Ingress"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if ingress_policies.is_empty() {
            return None;
        }

        let blocked_ports = service_ports
            .iter()
            .filter(|port| {
                let resolved_ports = resolve_service_target_ports(port, backend);
                !client_pods.iter().any(|client| {
                    ingress_allows_source_pod(
                        &ingress_policies,
                        client,
                        backend,
                        &resolved_ports,
                        namespaces_by_name,
                    )
                })
            })
            .map(service_port_label)
            .collect::<Vec<_>>();

        if blocked_ports.len() != service_ports.len() {
            return None;
        }

        let primary_policy = select_primary_policy(&ingress_policies, "ingress");
        let detail = format!(
            "path=Service/{}/{} -> Pod/{}/{} blocked_ports=[{}] reason=no matching internal client peers",
            service.namespace,
            service.name,
            backend.namespace,
            backend.name,
            blocked_ports.join(","),
        );
        return Some((primary_policy, detail));
    }

    None
}

fn service_backend_pods<'a>(service: &ServiceState, all_pods: &'a [PodState]) -> Vec<&'a PodState> {
    let pod_by_name = all_pods
        .iter()
        .map(|pod| ((pod.namespace.clone(), pod.name.clone()), pod))
        .collect::<BTreeMap<_, _>>();
    service
        .matched_pods
        .iter()
        .filter_map(|pod_name| {
            pod_by_name
                .get(&(service.namespace.clone(), pod_name.clone()))
                .copied()
        })
        .collect::<Vec<_>>()
}

fn normalize_service_ports(service: &ServiceState) -> Vec<ServicePortState> {
    if service.ports.is_empty() {
        return Vec::new();
    }
    service.ports.clone()
}

fn policy_selects_pod(policy: &NetworkPolicyState, pod: &PodState) -> bool {
    if policy.namespace != pod.namespace {
        return false;
    }
    selector_matches(
        &policy.pod_selector,
        &policy.pod_selector_expressions,
        &pod.pod_labels,
    )
}

fn policy_has_type(policy: &NetworkPolicyState, direction: &str) -> bool {
    policy.policy_types.iter().any(|kind| kind == direction)
}

fn ingress_allows_source_pod(
    policies: &[&NetworkPolicyState],
    source_pod: &PodState,
    destination_pod: &PodState,
    destination_ports: &[ResolvedPort],
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    for policy in policies {
        for rule in &policy.ingress_rules {
            if !rule_peers_allow_source_pod(
                &rule.peers,
                source_pod,
                &policy.namespace,
                namespaces_by_name,
            ) {
                continue;
            }
            if rule_ports_allow_destination_ports(&rule.ports, destination_ports) {
                return true;
            }
        }
    }
    let _ = destination_pod;
    false
}

fn ingress_allows_external(
    policies: &[&NetworkPolicyState],
    _destination_pod: &PodState,
    destination_ports: &[ResolvedPort],
    _namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    for policy in policies {
        for rule in &policy.ingress_rules {
            if !rule_peers_allow_external(&rule.peers) {
                continue;
            }
            if rule_ports_allow_destination_ports(&rule.ports, destination_ports) {
                return true;
            }
        }
    }
    false
}

fn egress_policies_allow_any_destination(
    policies: &[&NetworkPolicyState],
    source_pod: &PodState,
    all_pods: &[PodState],
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    if policies.iter().any(|policy| {
        policy.egress_rules.iter().any(|rule| {
            rule_peers_allow_external(&rule.peers)
                && (rule.ports.is_empty() || rule.ports.iter().any(|port| port.port.is_none()))
        })
    }) {
        return true;
    }

    for destination_pod in all_pods
        .iter()
        .filter(|pod| !(pod.namespace == source_pod.namespace && pod.name == source_pod.name))
    {
        let destination_ports = resolve_pod_ports(destination_pod);
        for policy in policies {
            for rule in &policy.egress_rules {
                if !rule_peers_allow_destination_pod(
                    &rule.peers,
                    destination_pod,
                    &policy.namespace,
                    namespaces_by_name,
                ) {
                    continue;
                }
                if rule_ports_allow_destination_ports(&rule.ports, &destination_ports) {
                    return true;
                }
            }
        }
    }
    false
}

fn rule_peers_allow_source_pod(
    peers: &[NetworkPolicyPeerState],
    source_pod: &PodState,
    policy_namespace: &str,
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    if peers.is_empty() {
        return true;
    }
    peers
        .iter()
        .any(|peer| peer_matches_pod(peer, source_pod, policy_namespace, namespaces_by_name))
}

fn rule_peers_allow_destination_pod(
    peers: &[NetworkPolicyPeerState],
    destination_pod: &PodState,
    policy_namespace: &str,
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    if peers.is_empty() {
        return true;
    }
    peers
        .iter()
        .any(|peer| peer_matches_pod(peer, destination_pod, policy_namespace, namespaces_by_name))
}

fn rule_peers_allow_external(peers: &[NetworkPolicyPeerState]) -> bool {
    if peers.is_empty() {
        return true;
    }
    peers.iter().any(ip_block_allows_external)
}

fn ip_block_allows_external(peer: &NetworkPolicyPeerState) -> bool {
    let Some(cidr) = peer.ip_block_cidr.as_deref() else {
        return false;
    };
    if cidr == "0.0.0.0/0"
        && peer
            .ip_block_except
            .iter()
            .any(|exception| exception == "0.0.0.0/0")
    {
        return false;
    }
    true
}

fn peer_matches_pod(
    peer: &NetworkPolicyPeerState,
    pod: &PodState,
    policy_namespace: &str,
    namespaces_by_name: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    if peer.ip_block_cidr.is_some() {
        return false;
    }

    let namespace_selector_present = !peer.namespace_selector.is_empty()
        || !peer.namespace_selector_expressions.is_empty()
        || peer.has_namespace_selector_expressions;
    let pod_selector_present = !peer.pod_selector.is_empty()
        || !peer.pod_selector_expressions.is_empty()
        || peer.has_pod_selector_expressions;

    let namespace_matches = if namespace_selector_present {
        let namespace_labels = namespaces_by_name
            .get(&pod.namespace)
            .cloned()
            .unwrap_or_else(|| {
                BTreeMap::from([(
                    "kubernetes.io/metadata.name".to_string(),
                    pod.namespace.clone(),
                )])
            });
        selector_matches(
            &peer.namespace_selector,
            &peer.namespace_selector_expressions,
            &namespace_labels,
        )
    } else if pod_selector_present {
        pod.namespace == policy_namespace
    } else {
        true
    };
    if !namespace_matches {
        return false;
    }

    if pod_selector_present {
        selector_matches(
            &peer.pod_selector,
            &peer.pod_selector_expressions,
            &pod.pod_labels,
        )
    } else {
        true
    }
}

fn rule_ports_allow_destination_ports(
    policy_ports: &[NetworkPolicyPortState],
    destination_ports: &[ResolvedPort],
) -> bool {
    if policy_ports.is_empty() {
        return true;
    }

    policy_ports.iter().any(|policy_port| {
        destination_ports.iter().any(|destination_port| {
            policy_port_matches_resolved_port(policy_port, destination_port)
        })
    })
}

fn policy_port_matches_resolved_port(
    policy_port: &NetworkPolicyPortState,
    destination_port: &ResolvedPort,
) -> bool {
    if let Some(protocol) = policy_port.protocol.as_deref() {
        if !protocol.eq_ignore_ascii_case(&destination_port.protocol) {
            return false;
        }
    }

    let Some(port_value) = policy_port.port.as_deref() else {
        return true;
    };

    if let Ok(start_port) = port_value.parse::<i32>() {
        if let Some(end_port) = policy_port.end_port {
            return destination_port
                .number
                .is_some_and(|port| (start_port..=end_port).contains(&port));
        }
        return destination_port.number == Some(start_port);
    }

    destination_port
        .name
        .as_deref()
        .is_some_and(|name| name == port_value)
}

fn service_port_label(service_port: &ServicePortState) -> String {
    format!("{}/{}", service_port.port, service_port.protocol)
}

fn select_primary_policy<'a>(
    policies: &[&'a NetworkPolicyState],
    direction: &str,
) -> &'a NetworkPolicyState {
    let mut sorted = policies.to_vec();
    sorted.sort_by(|a, b| {
        let a_default_deny = match direction {
            "ingress" => a.default_deny_ingress,
            "egress" => a.default_deny_egress,
            _ => false,
        };
        let b_default_deny = match direction {
            "ingress" => b.default_deny_ingress,
            "egress" => b.default_deny_egress,
            _ => false,
        };
        b_default_deny
            .cmp(&a_default_deny)
            .then_with(|| a.name.cmp(&b.name))
    });
    sorted[0]
}

#[derive(Debug, Clone)]
struct ResolvedPort {
    protocol: String,
    name: Option<String>,
    number: Option<i32>,
}

fn resolve_service_target_ports(
    service_port: &ServicePortState,
    pod: &PodState,
) -> Vec<ResolvedPort> {
    if let Some(target_port) = service_port.target_port.as_deref() {
        if let Ok(number) = target_port.parse::<i32>() {
            return vec![ResolvedPort {
                protocol: service_port.protocol.clone(),
                name: service_port.name.clone(),
                number: Some(number),
            }];
        }
        let matches = pod
            .ports
            .iter()
            .filter(|pod_port| {
                pod_port
                    .name
                    .as_deref()
                    .is_some_and(|name| name == target_port)
                    && pod_port
                        .protocol
                        .eq_ignore_ascii_case(&service_port.protocol)
            })
            .map(|pod_port| ResolvedPort {
                protocol: pod_port.protocol.clone(),
                name: pod_port.name.clone(),
                number: Some(pod_port.container_port),
            })
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            return matches;
        }
        return vec![ResolvedPort {
            protocol: service_port.protocol.clone(),
            name: Some(target_port.to_string()),
            number: None,
        }];
    }

    vec![ResolvedPort {
        protocol: service_port.protocol.clone(),
        name: service_port.name.clone(),
        number: Some(service_port.port),
    }]
}

fn resolve_pod_ports(pod: &PodState) -> Vec<ResolvedPort> {
    if pod.ports.is_empty() {
        return vec![ResolvedPort {
            protocol: "TCP".to_string(),
            name: None,
            number: None,
        }];
    }

    pod.ports
        .iter()
        .map(|port| ResolvedPort {
            protocol: port.protocol.clone(),
            name: port.name.clone(),
            number: Some(port.container_port),
        })
        .collect()
}

fn namespace_labels_by_name(ctx: &AnalysisContext) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut labels = ctx
        .namespaces
        .iter()
        .map(|ns| {
            let mut ns_labels = ns.labels.clone();
            ns_labels
                .entry("kubernetes.io/metadata.name".to_string())
                .or_insert_with(|| ns.name.clone());
            (ns.name.clone(), ns_labels)
        })
        .collect::<BTreeMap<_, _>>();
    for pod in &ctx.pods {
        labels.entry(pod.namespace.clone()).or_insert_with(|| {
            BTreeMap::from([(
                "kubernetes.io/metadata.name".to_string(),
                pod.namespace.clone(),
            )])
        });
    }
    labels
}

fn selector_matches(
    selector: &BTreeMap<String, String>,
    expressions: &[LabelSelectorRequirementState],
    labels: &BTreeMap<String, String>,
) -> bool {
    if !selector_matches_labels(selector, labels) {
        return false;
    }
    selector_expressions_match(expressions, labels)
}

fn selector_matches_labels(
    selector: &BTreeMap<String, String>,
    labels: &BTreeMap<String, String>,
) -> bool {
    selector
        .iter()
        .all(|(key, value)| labels.get(key) == Some(value))
}

fn selector_expressions_match(
    expressions: &[LabelSelectorRequirementState],
    labels: &BTreeMap<String, String>,
) -> bool {
    expressions.iter().all(|expression| {
        let value = labels.get(&expression.key);
        match expression.operator.as_str() {
            "In" => value.is_some_and(|current| expression.values.contains(current)),
            "NotIn" => value.is_none_or(|current| !expression.values.contains(current)),
            "Exists" => value.is_some(),
            "DoesNotExist" => value.is_none(),
            _ => false,
        }
    })
}

fn pvc_status_by_name(
    ctx: &AnalysisContext,
) -> BTreeMap<(String, String), (DependencyStatus, String)> {
    ctx.persistent_volume_claims
        .iter()
        .map(|pvc| {
            let (status, detail) = if !pvc.exists {
                (DependencyStatus::Missing, "PVC missing".to_string())
            } else if pvc.phase == "Unknown" {
                (DependencyStatus::Unknown, "PVC phase unknown".to_string())
            } else {
                (
                    DependencyStatus::Present,
                    format!("PVC phase={}", pvc.phase),
                )
            };
            ((pvc.namespace.clone(), pvc.name.clone()), (status, detail))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use types::{
        AnalysisContextBuilder, ContainerLifecycleState, ContainerState, DependencyStatus,
        DeploymentState, PersistentVolumeClaimState, PodDependency, PodDependencyKind,
        PodSchedulingState, PodState, ReplicaSetState, ServiceState,
    };

    use crate::{DependencyGraphBuilder, Relation, ResourceId};

    fn sample_pod() -> PodState {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "payments-api".to_string());

        PodState {
            name: "payments-api".to_string(),
            namespace: "prod".to_string(),
            phase: "Running".to_string(),
            restart_count: 0,
            controller_kind: Some("ReplicaSet".to_string()),
            controller_name: Some("payments-api-rs".to_string()),
            node: "worker-1".to_string(),
            pod_labels: labels,
            scheduling: PodSchedulingState {
                unschedulable: false,
                reason: None,
                message: None,
            },
            service_selectors: vec![],
            container_states: vec![ContainerState {
                name: "api".to_string(),
                restart_count: 0,
                state: ContainerLifecycleState::Running,
                last_termination_reason: None,
                last_termination_exit_code: None,
            }],
            dependencies: vec![
                PodDependency {
                    kind: PodDependencyKind::Secret,
                    name: "db-config".to_string(),
                    status: DependencyStatus::Missing,
                },
                PodDependency {
                    kind: PodDependencyKind::ConfigMap,
                    name: "app-config".to_string(),
                    status: DependencyStatus::Present,
                },
            ],
            persistent_volume_claims: vec!["data-volume".to_string()],
            ports: vec![],
        }
    }

    #[test]
    fn builds_mvp_dependency_edges() {
        let pod = sample_pod();
        let service = ServiceState {
            name: "payments".to_string(),
            namespace: "prod".to_string(),
            selector: BTreeMap::new(),
            matched_pods: vec!["payments-api".to_string()],
            ports: vec![],
        };
        let pvc = PersistentVolumeClaimState {
            name: "data-volume".to_string(),
            namespace: "prod".to_string(),
            exists: true,
            phase: "Bound".to_string(),
            volume_name: Some("pv-data-volume".to_string()),
            storage_class_name: Some("gp3".to_string()),
        };
        let ctx = AnalysisContextBuilder::new()
            .with_pods(vec![pod])
            .with_services(vec![service])
            .with_persistent_volume_claims(vec![pvc])
            .with_storage_classes(vec![types::StorageClassState {
                name: "gp3".to_string(),
                exists: true,
            }])
            .with_replica_sets(vec![ReplicaSetState {
                name: "payments-api-rs".to_string(),
                namespace: "prod".to_string(),
                selector: BTreeMap::new(),
                owner_deployment: Some("payments-api".to_string()),
            }])
            .with_deployments(vec![DeploymentState {
                name: "payments-api".to_string(),
                namespace: "prod".to_string(),
                selector: BTreeMap::new(),
            }])
            .build();

        let graph = DependencyGraphBuilder::from_context(&ctx);

        assert!(graph.has_relation(
            &ResourceId::pod("prod", "payments-api"),
            &ResourceId::secret("prod", "db-config"),
            Relation::UsesSecret
        ));
        assert!(graph.has_relation(
            &ResourceId::pod("prod", "payments-api"),
            &ResourceId::config_map("prod", "app-config"),
            Relation::UsesConfigMap
        ));
        assert!(graph.has_relation(
            &ResourceId::pod("prod", "payments-api"),
            &ResourceId::node("worker-1"),
            Relation::ScheduledOnNode
        ));
        assert!(graph.has_relation(
            &ResourceId::pod("prod", "payments-api"),
            &ResourceId::persistent_volume_claim("prod", "data-volume"),
            Relation::MountsPersistentVolumeClaim
        ));
        assert!(graph.has_relation(
            &ResourceId::service("prod", "payments"),
            &ResourceId::pod("prod", "payments-api"),
            Relation::RoutesToPod
        ));
        assert!(graph.has_relation(
            &ResourceId::persistent_volume_claim("prod", "data-volume"),
            &ResourceId::storage_class("gp3"),
            Relation::UsesStorageClass
        ));
    }
}
