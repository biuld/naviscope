use crate::model::graph::{BuildElement, CodeElement, GraphNode};
use serde::Serialize;

/// 节点的摘要信息，旨在为 Agent 提供精简的上下文
#[derive(Serialize, Debug)]
pub struct NodeSummary {
    pub fqn: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl From<&GraphNode> for NodeSummary {
    fn from(node: &GraphNode) -> Self {
        let location = node.file_path().map(|path| {
            if let Some(range) = node.range() {
                return format!("{}:{}:{}", path.display(), range.start_line + 1, range.start_col + 1);
            }
            path.display().to_string()
        });

        let signature = match node {
            GraphNode::Code(code_el) => match code_el {
                CodeElement::Java { element, .. } => match element {
                    crate::model::lang::java::JavaElement::Method(m) => {
                        let params = m
                            .parameters
                            .iter()
                            .map(|p| format!("{} {}", p.type_name, p.name))
                            .collect::<Vec<_>>()
                            .join(", ");
                        Some(format!("{}({}) -> {}", m.name, params, m.return_type))
                    }
                    crate::model::lang::java::JavaElement::Field(f) => {
                        Some(format!("{} {}", f.type_name, f.name))
                    }
                    _ => None,
                },
            },
            GraphNode::Build(build_el) => match build_el {
                BuildElement::Gradle { element, .. } => match element {
                    crate::model::lang::gradle::GradleElement::Dependency(d) => {
                        Some(format!("{}:{}:{}", d.group, d.name, d.version))
                    }
                    _ => None,
                },
            },
        };

        Self {
            fqn: node.fqn(),
            name: node.name().to_string(),
            kind: node.kind().to_string(),
            signature,
            location,
        }
    }
}
