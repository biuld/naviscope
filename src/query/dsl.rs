use crate::model::graph::{EdgeType, NodeKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    /// List members or structure (Rich Listing)
    Ls {
        /// Target node FQN, defaults to project modules if null
        fqn: Option<String>,
        #[serde(default)]
        kind: Vec<NodeKind>,
        #[serde(default)]
        modifiers: Vec<String>,
    },

    /// Search for symbols
    Grep {
        pattern: String,
        #[serde(default)]
        kind: Vec<NodeKind>,
        #[serde(default = "default_limit")]
        limit: usize,
    },

    /// Inspect node details (Source & Metadata)
    Cat { fqn: String },

    /// Find dependencies (outgoing) or dependents (incoming)
    Deps {
        fqn: String,
        /// If true, find incoming dependencies (who depends on me).
        /// If false (default), find outgoing dependencies (who do I depend on).
        #[serde(default)]
        rev: bool,
        #[serde(default)]
        edge_types: Vec<EdgeType>,
    },
}

fn default_limit() -> usize {
    20
}
