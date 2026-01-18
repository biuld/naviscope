use crate::model::graph::EdgeType;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    Grep {
        /// Search pattern (simple string or regex)
        pattern: String,
        /// Optional: Filter by type. 
        /// Valid Java kinds: ["class", "interface", "enum", "annotation", "method", "field"]
        /// Valid Build kinds: ["package", "dependency"]
        #[serde(default)]
        kind: Vec<String>,
        #[serde(default = "default_limit")]
        limit: usize,
    },

    Ls {
        /// Target node FQN, defaults to project modules if null
        fqn: Option<String>,
        /// Optional: Filter by type. 
        /// Valid Java kinds: ["class", "interface", "enum", "annotation", "method", "field"]
        /// Valid Build kinds: ["package", "dependency"]
        #[serde(default)]
        kind: Vec<String>,
    },

    Inspect {
        fqn: String,
    },

    Incoming {
        fqn: String,
        #[serde(default)]
        edge_type: Vec<EdgeType>,
    },

    Outgoing {
        fqn: String,
        #[serde(default)]
        edge_type: Vec<EdgeType>,
    },
}

fn default_limit() -> usize {
    20
}
