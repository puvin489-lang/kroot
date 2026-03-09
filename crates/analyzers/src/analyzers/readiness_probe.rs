use crate::{AnalysisInput, Analyzer, GraphAnalyzer};
use std::collections::BTreeSet;
use types::{AnalysisContext, Diagnosis, Remediation, Severity};

pub struct FailedReadinessProbeAnalyzer;

impl Analyzer for FailedReadinessProbeAnalyzer {
    fn analyze(&self, ctx: &AnalysisContext) -> Option<Diagnosis> {
        let mut evidence = Vec::new();
        let mut resources = BTreeSet::new();

        for event in &ctx.events {
            if event.involved_kind != "Pod" {
                continue;
            }
            if !event.message.contains("Readiness probe failed") {
                continue;
            }
            resources.insert(format!("Pod/{}/{}", event.namespace, event.involved_name));

            evidence.push(format!(
                "pod={}/{} reason={} message={}",
                event.namespace, event.involved_name, event.reason, event.message
            ));
        }

        if evidence.is_empty() {
            return None;
        }
        let resource = if resources.len() == 1 {
            resources
                .into_iter()
                .next()
                .unwrap_or_else(|| "Pods/*".to_string())
        } else {
            "Pods/*".to_string()
        };

        Some(Diagnosis {
            severity: Severity::Warning,
            confidence: 0.88,
            resource,
            message: "Readiness probe failures detected".to_string(),
            root_cause: "Pod is running but failing readiness checks".to_string(),
            evidence,
            remediation: Some(Remediation {
                summary: "Fix readiness endpoint behavior and dependent startup conditions"
                    .to_string(),
                steps: vec![
                    "Validate readiness probe path/port/command and timeout thresholds".to_string(),
                    "Ensure app dependencies are available before reporting ready".to_string(),
                    "Increase initial delay if startup is consistently slow".to_string(),
                ],
                commands: vec!["kubectl describe pod <pod> -n <namespace>".to_string()],
            }),
        })
    }
}

impl GraphAnalyzer for FailedReadinessProbeAnalyzer {
    fn analyze_graph(&self, input: &AnalysisInput<'_>) -> Option<Diagnosis> {
        self.analyze(input.context)
    }
}
