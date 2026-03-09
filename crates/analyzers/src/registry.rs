use crate::analyzers::{
    CrashLoopBackOffAnalyzer, FailedLivenessProbeAnalyzer, FailedMountPvcAnalyzer,
    FailedReadinessProbeAnalyzer, ImagePullBackOffAnalyzer, MissingConfigMapAnalyzer,
    MissingSecretAnalyzer, NetworkReachabilityAnalyzer, NodeNotReadyAnalyzer, OOMKilledAnalyzer,
    ServiceSelectorMismatchAnalyzer, UnschedulableAnalyzer,
};
use crate::{Analyzer, GraphAnalyzer};

pub fn default_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![]
}

pub fn default_graph_analyzers() -> Vec<Box<dyn GraphAnalyzer>> {
    vec![
        Box::new(CrashLoopBackOffAnalyzer),
        Box::new(ImagePullBackOffAnalyzer),
        Box::new(OOMKilledAnalyzer),
        Box::new(UnschedulableAnalyzer),
        Box::new(NodeNotReadyAnalyzer),
        Box::new(FailedReadinessProbeAnalyzer),
        Box::new(FailedLivenessProbeAnalyzer),
        Box::new(FailedMountPvcAnalyzer),
        Box::new(NetworkReachabilityAnalyzer),
        Box::new(MissingSecretAnalyzer),
        Box::new(MissingConfigMapAnalyzer),
        Box::new(ServiceSelectorMismatchAnalyzer),
    ]
}
