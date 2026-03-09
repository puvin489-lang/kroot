use crate::{AnalysisInput, GraphAnalyzer};
use graph::{Relation, ResourceKind};
use std::collections::BTreeSet;
use types::{DependencyStatus, Diagnosis, Remediation, Severity};

pub struct MissingConfigMapAnalyzer;

impl GraphAnalyzer for MissingConfigMapAnalyzer {
    fn analyze_graph(&self, input: &AnalysisInput<'_>) -> Option<Diagnosis> {
        let mut missing = Vec::new();
        for (pod, config_map, meta) in input
            .graph
            .relations_with_status(Relation::UsesConfigMap, DependencyStatus::Missing)
        {
            if pod.kind == ResourceKind::Pod && config_map.kind == ResourceKind::ConfigMap {
                let pod_namespace = pod.namespace.unwrap_or_else(|| "default".to_string());
                let pod_name = pod.name;
                let config_map_name = config_map.name;
                let mut evidence_item = format!(
                    "Pod/{pod_namespace}/{pod_name} -> ConfigMap/{config_map_name} -> ConfigMap missing"
                );
                if let Some(source) = meta.source {
                    evidence_item.push_str(&format!(" source={source}"));
                }
                if let Some(detail) = meta.detail {
                    evidence_item.push_str(&format!(" detail={detail}"));
                }
                missing.push((pod_namespace, pod_name, config_map_name, evidence_item));
            }
        }

        missing.sort();
        missing.dedup();

        if missing.is_empty() {
            return None;
        }

        let mut resources = BTreeSet::new();
        for (namespace, pod_name, _, _) in &missing {
            resources.insert(format!("Pod/{namespace}/{pod_name}"));
        }
        let resource = if resources.len() == 1 {
            resources
                .into_iter()
                .next()
                .unwrap_or_else(|| "Pods/*".to_string())
        } else {
            "Pods/*".to_string()
        };

        let root_cause = if missing.len() == 1 {
            format!(
                "Pod failing because configmap {} does not exist",
                missing[0].2
            )
        } else {
            format!(
                "Pod failing because {} referenced configmaps do not exist",
                missing.len()
            )
        };
        let evidence = missing
            .iter()
            .map(|(_, _, _, evidence)| evidence.clone())
            .collect::<Vec<_>>();

        Some(Diagnosis {
            severity: Severity::Critical,
            confidence: 0.97,
            resource,
            message: "Missing ConfigMap dependency detected".to_string(),
            root_cause,
            evidence,
            remediation: Some(Remediation {
                summary: "Create the missing ConfigMap or update workload references".to_string(),
                steps: vec![
                    "Create the referenced configmap in the same namespace as the failing pod"
                        .to_string(),
                    "Validate config key names match envFrom/env/volume references".to_string(),
                    "Restart rollout to ensure pods consume the corrected config".to_string(),
                ],
                commands: vec![
                    "kubectl create configmap <name> -n <namespace> --from-literal=<key>=<value>"
                        .to_string(),
                    "kubectl rollout restart deployment/<deployment> -n <namespace>".to_string(),
                ],
            }),
        })
    }
}
