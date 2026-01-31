use crate::model::GradleElement;
use naviscope_api::models::GraphNode;
use naviscope_core::plugin::LanguageFeatureProvider;

pub struct GradleFeatureProvider;

impl GradleFeatureProvider {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageFeatureProvider for GradleFeatureProvider {
    fn detail_view(&self, node: &GraphNode) -> Option<String> {
        if &*node.lang != "buildfile" {
            return None;
        }

        let element = serde_json::from_value::<GradleElement>(node.metadata.clone()).ok()?;

        match element {
            GradleElement::Module(_) => Some(format!("**Gradle Module**: {}", node.name)),
            GradleElement::Dependency(d) => {
                let group = d.group.as_deref().unwrap_or("?");
                let version = d.version.as_deref().unwrap_or("?");
                if d.is_project {
                    Some(format!("**Project Dependency**: {}", node.name))
                } else {
                    Some(format!(
                        "**External Dependency**: {}:{}:{}",
                        group, node.name, version
                    ))
                }
            }
        }
    }

    fn signature(&self, node: &GraphNode) -> Option<String> {
        if &*node.lang != "buildfile" {
            return None;
        }

        let element = serde_json::from_value::<GradleElement>(node.metadata.clone()).ok()?;

        match element {
            GradleElement::Dependency(d) => {
                let group = d.group.as_deref().unwrap_or("?");
                let version = d.version.as_deref().unwrap_or("?");
                Some(format!("{}:{}:{}", group, node.name, version))
            }
            _ => None,
        }
    }

    fn modifiers(&self, _node: &GraphNode) -> Vec<String> {
        vec![]
    }
}
