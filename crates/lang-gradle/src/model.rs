use lasso::Key;
use naviscope_api::models::graph::NodeMetadata;
use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Debug, Serialize, Deserialize)]
pub struct GradleNodeMetadata {
    pub element: GradleStorageElement,
}

impl GradleNodeMetadata {
    pub fn new(element: GradleStorageElement) -> Self {
        Self { element }
    }

    pub fn detail_view(
        &self,
        fqns: &dyn naviscope_api::models::symbol::FqnReader,
    ) -> Option<String> {
        match &self.element {
            GradleStorageElement::Dependency(d) => {
                let mut detail = String::new();
                if let Some(group_sid) = d.group_sid {
                    detail.push_str(fqns.resolve_atom(naviscope_api::models::symbol::Symbol(
                        lasso::Spur::try_from_usize(group_sid as usize).unwrap(),
                    )));
                }
                if let Some(version_sid) = d.version_sid {
                    if !detail.is_empty() {
                        detail.push(':');
                    }
                    detail.push_str(fqns.resolve_atom(naviscope_api::models::symbol::Symbol(
                        lasso::Spur::try_from_usize(version_sid as usize).unwrap(),
                    )));
                }
                if d.is_project {
                    detail.push_str(" (Project)");
                }
                if detail.is_empty() {
                    None
                } else {
                    Some(detail)
                }
            }
            _ => None,
        }
    }
}

impl NodeMetadata for GradleNodeMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// The IndexMetadata trait and its implementation are removed as per the instruction
// to remove unused imports and the provided snippet which removes IndexMetadata
// from the naviscope_plugin import.
// If this causes compilation errors, the user's instruction or snippet was incomplete.
// impl IndexMetadata for GradleNodeMetadata {
//     fn as_any(&self) -> &dyn Any {
//         self
//     }

//     fn intern(&self, _interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata> {
//         // GradleNodeMetadata is already interned (contains GradleStorageElement)
//         Arc::new(GradleNodeMetadata {
//             element: self.element.clone(),
//         })
//     }
// }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GradleElement {
    Module(GradleModule),
    Dependency(GradleDependency),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GradleStorageElement {
    Module(GradleModuleStorage),
    Dependency(GradleDependencyStorage),
}

impl GradleElement {}

impl GradleStorageElement {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleModule {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleModuleStorage {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleDependency {
    pub group: Option<String>,
    pub version: Option<String>,
    pub is_project: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleDependencyStorage {
    pub group_sid: Option<u32>,
    pub version_sid: Option<u32>,
    pub is_project: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleParseResult {
    pub dependencies: Vec<RawGradleDependency>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RawGradleDependency {
    pub group: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub is_project: bool,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleSettings {
    pub root_project_name: Option<String>,
    pub included_projects: Vec<String>,
}
