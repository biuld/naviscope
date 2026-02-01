use naviscope_api::models::{DisplayGraphNode, NodeKind};
use tabled::Tabled;

/// A terminal-optimized view of a GraphNode (Detailed)
#[derive(Tabled)]
pub struct ShellNodeView {
    pub kind: String,
    pub name: String,
    pub relation: String,
    pub signature: String,
    pub location: String,
    pub fqn: String,
}

/// A short view of a GraphNode
#[derive(Tabled)]
pub struct ShellNodeViewShort {
    pub kind: String,
    pub name: String,
}

impl ShellNodeView {
    pub fn from_node(
        node: &DisplayGraphNode,
        relation: Option<String>,
    ) -> Self {
        let location = node
            .location
            .as_ref()
            .map(|loc| {
                // DisplaySymbolLocation has String path
                let path = std::path::Path::new(&loc.path);
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("-");
                format!("{}:{}", filename, loc.range.start_line + 1)
            })
            .unwrap_or_else(|| "-".to_string());

        let is_container = matches!(
            node.kind,
            NodeKind::Project
                | NodeKind::Module
                | NodeKind::Package
                | NodeKind::Class
                | NodeKind::Interface
                | NodeKind::Enum
                | NodeKind::Annotation
        );

        let name = if is_container {
            format!("{}/", node.name)
        } else {
            node.name.clone()
        };

        // Use pre-filled signature in DisplayGraphNode
        let signature = node.signature.clone().unwrap_or_else(|| {
            // Fallback for nodes without specific signature
            match node.kind {
                NodeKind::Project => "Project".to_string(),
                _ => "-".to_string(),
            }
        });

        Self {
            fqn: shorten_fqn(&node.id),
            name,
            kind: node.kind.to_string(),
            relation: relation.unwrap_or_else(|| "-".to_string()),
            signature,
            location,
        }
    }
}

pub fn shorten_fqn(fqn: &str) -> String {
    let separator = if fqn.contains("::") { "::" } else { "." };
    let parts: Vec<&str> = fqn.split(separator).collect();
    if parts.len() <= 2 {
        return fqn.to_string();
    }
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i < parts.len() - 2 {
            if let Some(c) = part.chars().next() {
                result.push(c);
                result.push_str(separator);
            }
        } else {
            result.push_str(part);
            if i < parts.len() - 1 {
                result.push_str(separator);
            }
        }
    }
    result
}

pub fn get_kind_weight(kind: &str) -> i32 {
    match kind.to_lowercase().as_str() {
        "package" => 1,
        "class" => 2,
        "interface" => 3,
        "enum" => 4,
        "annotation" => 5,
        "constructor" => 6,
        "method" => 7,
        "field" => 8,
        _ => 99,
    }
}
