use crate::model::{GraphEdge, NodeKind, Range};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct StorageGraph {
    pub version: u32,
    pub string_pool: Vec<String>,
    pub path_pool: Vec<String>,
    pub nodes: Vec<StorageNode>,
    pub edges: Vec<StorageEdge>,
    pub fqn_index: Vec<(u32, u32)>,       // (StringID, NodeIdx)
    pub name_index: Vec<(u32, Vec<u32>)>, // (StringID, Vec<NodeIdx>)
    pub file_index: Vec<(u32, StorageFileEntry)>, // (PathID, Entry)
    pub reference_index: Vec<(u32, Vec<u32>)>, // (StringID, Vec<PathID>)
}

#[derive(Serialize, Deserialize)]
pub struct StorageNode {
    pub id_sid: u32,
    pub name_sid: u32,
    pub kind: NodeKind,
    pub lang_sid: u32,
    pub location: Option<StorageLocation>,
    pub metadata: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct StorageLocation {
    pub path_id: u32,
    pub range: Range,
    pub selection_range: Option<Range>,
}

#[derive(Serialize, Deserialize)]
pub struct StorageEdge {
    pub from: u32,
    pub to: u32,
    pub data: GraphEdge,
}

#[derive(Serialize, Deserialize)]
pub struct StorageFileEntry {
    pub metadata: crate::project::source::SourceFile,
    pub nodes: Vec<u32>,
}
