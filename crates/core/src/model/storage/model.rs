use crate::model::FqnStorage;
use crate::model::{GraphEdge, NodeKind, Range};
use lasso::{Key, ThreadedRodeo};
use naviscope_api::models::graph::NodeSource;
use naviscope_api::models::symbol::Symbol;
use naviscope_plugin::FqnInterner;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Context for interning and resolving symbols during storage conversion.
pub trait StorageContext: crate::model::metadata::SymbolInterner {
    fn intern_path(&mut self, p: &std::path::Path) -> u32;
    fn resolve_str(&self, sid: u32) -> &str;
    fn resolve_path(&self, pid: u32) -> &std::path::Path;
}

pub struct GenericStorageContext {
    pub rodeo: Arc<ThreadedRodeo>,
}

impl naviscope_plugin::StorageContext for GenericStorageContext {
    fn interner(&mut self) -> &mut dyn FqnInterner {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl naviscope_api::models::symbol::FqnReader for GenericStorageContext {
    fn resolve_node(
        &self,
        _id: naviscope_api::models::symbol::FqnId,
    ) -> Option<naviscope_api::models::symbol::FqnNode> {
        // Storage context doesn't need to resolve FQN nodes
        None
    }

    fn resolve_atom(&self, atom: Symbol) -> &str {
        self.rodeo.resolve(&atom.0)
    }
}

impl FqnInterner for GenericStorageContext {
    fn intern_atom(&self, name: &str) -> Symbol {
        Symbol(self.rodeo.get_or_intern(name))
    }

    fn intern_node(
        &self,
        _parent: Option<naviscope_api::models::symbol::FqnId>,
        _name: &str,
        _kind: naviscope_api::models::graph::NodeKind,
    ) -> naviscope_api::models::symbol::FqnId {
        // This is a simplified implementation for storage context
        // In practice, this should delegate to the actual FqnManager
        // For now, we just create a flat ID
        naviscope_api::models::symbol::FqnId(0)
    }

    fn intern_node_id(
        &self,
        _id: &naviscope_api::models::symbol::NodeId,
    ) -> naviscope_api::models::symbol::FqnId {
        naviscope_api::models::symbol::FqnId(0)
    }
}

impl crate::model::metadata::SymbolInterner for GenericStorageContext {
    fn intern_str(&mut self, s: &str) -> u32 {
        self.rodeo.get_or_intern(s).into_usize() as u32
    }
}

impl StorageContext for GenericStorageContext {
    fn intern_path(&mut self, p: &std::path::Path) -> u32 {
        let s = p.to_string_lossy();
        crate::model::metadata::SymbolInterner::intern_str(self, s.as_ref())
    }

    fn resolve_str(&self, sid: u32) -> &str {
        use lasso::{Key, Spur};
        let spur = Spur::try_from_usize(sid as usize).unwrap();
        // SAFE: ThreadedRodeo's resolve returns a &str that lives as long as the rodeo.
        // Since we hold the Arc, this is fine conceptually, but we need to cheat the borrow checker
        // or return an owned String.
        // Actually, resolve returns &'static str if we're not careful? No.
        // For simplicity during transition, let's use an unsafe cast or return String if needed.
        // But ThreadedRodeo::resolve returns &'a str.
        let s: &str = self.rodeo.resolve(&spur);
        unsafe { std::mem::transmute(s) }
    }

    fn resolve_path(&self, pid: u32) -> &std::path::Path {
        use lasso::{Key, Spur};
        let spur = Spur::try_from_usize(pid as usize).unwrap();
        let s: &str = self.rodeo.resolve(&spur);
        std::path::Path::new(unsafe { std::mem::transmute::<&str, &'static str>(s) })
    }
}

#[derive(Serialize, Deserialize)]
pub struct StorageGraph {
    pub version: u32,
    pub fqns: FqnStorage,
    pub nodes: Vec<StorageNode>,
    pub edges: Vec<StorageEdge>,
    pub fqn_index: Vec<(u32, u32)>,               // (FqnId, NodeIdx)
    pub name_index: Vec<(u32, Vec<u32>)>,         // (Symbol, Vec<NodeIdx>)
    pub file_index: Vec<(u32, StorageFileEntry)>, // (Symbol, Entry)
    pub reference_index: Vec<(u32, Vec<u32>)>,    // (Symbol, Vec<Symbol>)
}

#[derive(Serialize, Deserialize)]
pub struct StorageNode {
    pub id_sid: u32,
    pub name_sid: u32,
    pub kind: NodeKind,
    pub lang_sid: u32,
    pub source: NodeSource,
    pub location: Option<StorageLocation>,
    pub metadata: Box<[u8]>,
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
    pub metadata: crate::model::source::SourceFile,
    pub nodes: Vec<u32>,
}
