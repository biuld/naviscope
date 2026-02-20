use super::graph::NodeKind;
use super::symbol::{FqnId, Symbol};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct FqnNode {
    pub parent: Option<FqnId>,
    pub name: Symbol,
    pub kind: NodeKind,
}

pub trait FqnReader {
    fn resolve_node(&self, id: FqnId) -> Option<FqnNode>;
    fn resolve_atom(&self, atom: Symbol) -> &str;
}
