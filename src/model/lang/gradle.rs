use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GradleElement {
    Package(GradlePackage),
    Dependency(GradleDependency),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradlePackage {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GradleDependency {
    pub group: String,
    pub name: String,
    pub version: String,
}
