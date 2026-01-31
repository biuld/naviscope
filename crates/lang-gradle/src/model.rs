use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GradleElement {
    Module(GradleModule),
    Dependency(GradleDependency),
}

impl GradleElement {
    pub fn id(&self) -> &str {
        match self {
            GradleElement::Module(m) => &m.id,
            GradleElement::Dependency(d) => &d.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            GradleElement::Module(m) => &m.name,
            GradleElement::Dependency(d) => &d.name,
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            GradleElement::Module(_) => "module",
            GradleElement::Dependency(_) => "dependency",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleModule {
    pub name: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleDependency {
    pub group: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub is_project: bool,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleParseResult {
    pub dependencies: Vec<GradleDependency>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleSettings {
    pub root_project_name: Option<String>,
    pub included_projects: Vec<String>,
}
