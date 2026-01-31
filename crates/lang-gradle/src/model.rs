use naviscope_core::engine::storage::model::StorageContext;
use serde::{Deserialize, Serialize};

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

impl GradleElement {
    pub fn intern(&self, ctx: &mut dyn StorageContext) -> GradleStorageElement {
        match self {
            GradleElement::Module(_) => GradleStorageElement::Module(GradleModuleStorage {}),
            GradleElement::Dependency(d) => GradleStorageElement::Dependency(GradleDependencyStorage {
                group_sid: d.group.as_ref().map(|s| ctx.intern_str(s)),
                version_sid: d.version.as_ref().map(|s| ctx.intern_str(s)),
                is_project: d.is_project,
            }),
        }
    }
}

impl GradleStorageElement {
    pub fn resolve(&self, ctx: &dyn StorageContext) -> GradleElement {
        match self {
            GradleStorageElement::Module(_) => GradleElement::Module(GradleModule {}),
            GradleStorageElement::Dependency(d) => GradleElement::Dependency(GradleDependency {
                group: d.group_sid.map(|sid| ctx.resolve_str(sid).to_string()),
                version: d.version_sid.map(|sid| ctx.resolve_str(sid).to_string()),
                is_project: d.is_project,
            }),
        }
    }
}

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
