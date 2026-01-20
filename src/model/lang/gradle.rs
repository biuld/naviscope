use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GradleElement {
    Package(GradlePackage),
    Dependency(GradleDependency),
}

impl GradleElement {
    pub fn id(&self) -> &str {
        match self {
            GradleElement::Package(p) => &p.id,
            GradleElement::Dependency(d) => &d.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            GradleElement::Package(p) => &p.name,
            GradleElement::Dependency(d) => &d.name,
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            GradleElement::Package(_) => "package",
            GradleElement::Dependency(_) => "dependency",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradlePackage {
    pub name: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleDependency {
    pub group: String,
    pub name: String,
    pub version: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleParseResult {
    pub dependencies: Vec<GradleDependency>,
}
