use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClassState {
    pub name: String,
    pub exists: bool,
}
