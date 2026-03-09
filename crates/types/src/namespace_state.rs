use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceState {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}
