use crate::{AnalysisInput, GraphAnalyzer};
use graph::{Relation, ResourceId};
use std::collections::BTreeSet;
use types::{Diagnosis, Remediation, Severity};

pub struct ServiceSelectorMismatchAnalyzer;

impl GraphAnalyzer for ServiceSelectorMismatchAnalyzer {
    fn analyze_graph(&self, input: &AnalysisInput<'_>) -> Option<Diagnosis> {
        let mut evidence = Vec::new();
        let mut resources = BTreeSet::new();

        for service in &input.context.services {
            if service.selector.is_empty() {
                continue;
            }

            let service_id = ResourceId::service(&service.namespace, &service.name);
            let routed_pods = input
                .graph
                .related_resources(&service_id, Relation::RoutesToPod);
            if !routed_pods.is_empty() {
                continue;
            }

            resources.insert(format!("Service/{}/{}", service.namespace, service.name));
            let selector = service
                .selector
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            evidence.push(format!(
                "service={}/{} selector=[{}] has no matched pods",
                service.namespace, service.name, selector
            ));
        }

        if evidence.is_empty() {
            return None;
        }
        let resource = if resources.len() == 1 {
            resources
                .into_iter()
                .next()
                .unwrap_or_else(|| "Services/*".to_string())
        } else {
            "Services/*".to_string()
        };

        Some(Diagnosis {
            severity: Severity::Warning,
            confidence: 0.90,
            resource,
            message: "Service selector mismatch detected".to_string(),
            root_cause: "Service selector does not match any pod labels".to_string(),
            evidence,
            remediation: Some(Remediation {
                summary: "Align Service selectors with workload pod labels".to_string(),
                steps: vec![
                    "Compare service selector keys/values against pod labels".to_string(),
                    "Update the service selector or workload labels to match".to_string(),
                    "Confirm endpoints are populated after reconciliation".to_string(),
                ],
                commands: vec![
                    "kubectl get svc <service> -n <namespace> -o yaml".to_string(),
                    "kubectl get pods -n <namespace> --show-labels".to_string(),
                    "kubectl get endpoints <service> -n <namespace>".to_string(),
                ],
            }),
        })
    }
}
