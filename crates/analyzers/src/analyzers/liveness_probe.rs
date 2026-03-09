use crate::{AnalysisInput, Analyzer, GraphAnalyzer};
use std::collections::BTreeSet;
use types::{AnalysisContext, Diagnosis, Remediation, Severity};

pub struct FailedLivenessProbeAnalyzer;

impl Analyzer for FailedLivenessProbeAnalyzer {
    fn analyze(&self, ctx: &AnalysisContext) -> Option<Diagnosis> {
        let mut evidence = Vec::new();
        let mut resources = BTreeSet::new();

        for event in &ctx.events {
            if event.involved_kind != "Pod" {
                continue;
            }
            if !event.message.contains("Liveness probe failed") {
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
            confidence: 0.90,
            resource,
            message: "Liveness probe failures detected".to_string(),
            root_cause: "Container is being restarted by failing liveness checks".to_string(),
            evidence,
            remediation: Some(Remediation {
                summary: "Tune liveness probes so healthy-but-slow containers are not killed"
                    .to_string(),
                steps: vec![
                    "Verify liveness endpoint/command is correct and stable".to_string(),
                    "Increase failureThreshold/timeoutSeconds/initialDelaySeconds as needed"
                        .to_string(),
                    "Fix application deadlock or crash behavior if probe failures are legitimate"
                        .to_string(),
                ],
                commands: vec!["kubectl describe pod <pod> -n <namespace>".to_string()],
            }),
        })
    }
}

impl GraphAnalyzer for FailedLivenessProbeAnalyzer {
    fn analyze_graph(&self, input: &AnalysisInput<'_>) -> Option<Diagnosis> {
        self.analyze(input.context)
    }
}
