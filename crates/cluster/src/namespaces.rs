use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, Client, api::ListParams};
use types::{AnalysisContextBuilder, NamespaceState};

use crate::collector::{ClusterResult, CollectInput, Collector};

pub struct NamespaceCollector;

impl Collector for NamespaceCollector {
    fn collect<'a>(
        &'a self,
        client: &'a Client,
        input: &'a CollectInput,
        builder: AnalysisContextBuilder,
    ) -> Pin<Box<dyn Future<Output = ClusterResult<AnalysisContextBuilder>> + 'a>> {
        Box::pin(async move {
            let namespaces = collect_namespaces(client, &input.namespace).await;
            Ok(builder.with_namespaces(namespaces))
        })
    }
}

async fn collect_namespaces(client: &Client, fallback_namespace: &str) -> Vec<NamespaceState> {
    let namespaces_api: Api<Namespace> = Api::all(client.clone());
    match namespaces_api.list(&ListParams::default()).await {
        Ok(list) => list
            .items
            .into_iter()
            .filter_map(|namespace| {
                let name = namespace.metadata.name?;
                let mut labels = namespace.metadata.labels.unwrap_or_default();
                labels
                    .entry("kubernetes.io/metadata.name".to_string())
                    .or_insert_with(|| name.clone());
                Some(NamespaceState { name, labels })
            })
            .collect(),
        Err(_) => vec![NamespaceState {
            name: fallback_namespace.to_string(),
            labels: BTreeMap::from([(
                "kubernetes.io/metadata.name".to_string(),
                fallback_namespace.to_string(),
            )]),
        }],
    }
}
