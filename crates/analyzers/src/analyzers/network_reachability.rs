use crate::{AnalysisInput, Analyzer, GraphAnalyzer};
use graph::{Relation, ResourceId, ResourceKind};
use std::collections::BTreeSet;
use types::{AnalysisContext, Diagnosis, Remediation, Severity};

pub struct NetworkReachabilityAnalyzer;

impl Analyzer for NetworkReachabilityAnalyzer {
    fn analyze(&self, _ctx: &AnalysisContext) -> Option<Diagnosis> {
        None
    }
}

impl GraphAnalyzer for NetworkReachabilityAnalyzer {
    fn analyze_graph(&self, input: &AnalysisInput<'_>) -> Option<Diagnosis> {
        let mut evidence = Vec::new();
        let mut impacted_resources = BTreeSet::new();
        let mut has_service_or_ingress_impact = false;

        let mut blocked = input.graph.relations(Relation::BlockedByNetworkPolicy);
        blocked.sort_by(|(from_a, to_a, _), (from_b, to_b, _)| {
            resource_label(from_a)
                .cmp(&resource_label(from_b))
                .then_with(|| resource_label(to_a).cmp(&resource_label(to_b)))
        });

        for (from, to, edge) in blocked {
            if to.kind != ResourceKind::NetworkPolicy {
                continue;
            }
            let from_label = resource_label(&from);
            let to_label = resource_label(&to);
            if matches!(from.kind, ResourceKind::Service | ResourceKind::Ingress) {
                has_service_or_ingress_impact = true;
            }

            impacted_resources.insert(from_label.clone());

            let mut line = format!("{from_label} -> {to_label}");
            if let Some(detail) = edge.detail {
                line.push_str(&format!(" ({detail})"));
            }
            if let Some(source) = edge.source {
                line.push_str(&format!(" source={source}"));
            }
            evidence.push(line);
        }

        if evidence.is_empty() {
            return None;
        }

        let resource = if impacted_resources.len() == 1 {
            impacted_resources
                .into_iter()
                .next()
                .unwrap_or_else(|| "NetworkPolicies/*".to_string())
        } else {
            "NetworkPolicies/*".to_string()
        };

        let severity = if has_service_or_ingress_impact {
            Severity::Critical
        } else {
            Severity::Warning
        };
        let confidence = if has_service_or_ingress_impact {
            0.90
        } else {
            0.84
        };

        Some(Diagnosis {
            severity,
            confidence,
            resource,
            message: "Network reachability blocked by NetworkPolicy".to_string(),
            root_cause: "Ingress/egress rules do not permit required peer and port communication"
                .to_string(),
            evidence,
            remediation: Some(Remediation {
                summary: "Allow required peer and port combinations in NetworkPolicy".to_string(),
                steps: vec![
                    "Identify blocked service or pod traffic paths from the evidence chain"
                        .to_string(),
                    "Add explicit ingress/egress peers and required ports for expected flows"
                        .to_string(),
                    "Re-test connectivity after applying policy updates".to_string(),
                ],
                commands: vec![
                    "kubectl get networkpolicy -A".to_string(),
                    "kubectl describe networkpolicy <policy> -n <namespace>".to_string(),
                ],
            }),
        })
    }
}

fn resource_label(resource: &ResourceId) -> String {
    match &resource.namespace {
        Some(namespace) => format!("{:?}/{namespace}/{}", resource.kind, resource.name),
        None => format!("{:?}/{}", resource.kind, resource.name),
    }
}
