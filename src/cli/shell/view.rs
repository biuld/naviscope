use naviscope::model::graph::{BuildElement, CodeElement, GraphNode, NodeKind};
use naviscope::model::signature::TypeRef;
use naviscope::model::lang::java::{JavaElement, JavaParameter};
use naviscope::model::lang::gradle::GradleElement;
use tabled::Tabled;
use std::path::PathBuf;

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
    pub fn from_node(node: &GraphNode, relation: Option<String>) -> Self {
        let location = node.file_path().map(|path: &PathBuf| {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("-");
            
            if let Some(range) = node.range() {
                return format!("{}:{}", filename, range.start_line + 1);
            }
            filename.to_string()
        }).unwrap_or_else(|| "-".to_string());

        let is_container = matches!(node.kind(), 
            NodeKind::Class | 
            NodeKind::Interface | 
            NodeKind::Enum |
            NodeKind::Annotation);
        
        let name = if is_container {
            format!("{}/", node.name())
        } else {
            node.name().to_string()
        };

        let signature = match node {
            GraphNode::Code(code_el) => match code_el {
                CodeElement::Java { element, .. } => match element {
                    JavaElement::Method(m) => {
                        if m.is_constructor {
                            let params_str = m.parameters.iter()
                                .map(|p| format!("{}", fmt_type(&p.type_ref)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{}({})", m.name, params_str)
                        } else {
                            fmt_shell_signature(&m.parameters, &m.return_type)
                        }
                    }
                    JavaElement::Field(f) => {
                        format!("{} {}", fmt_type(&f.type_ref), f.name)
                    }
                    _ => "-".to_string(),
                },
            },
            GraphNode::Build(build_el) => match build_el {
                BuildElement::Gradle { element, .. } => match element {
                GradleElement::Dependency(d) => {
                    let group = d.group.as_deref().unwrap_or("?");
                    let version = d.version.as_deref().unwrap_or("?");
                    format!("{}:{}:{}", group, d.name, version)
                }
                    _ => "-".to_string(),
                },
            },
        };

        Self {
            fqn: shorten_fqn(node.fqn()),
            name,
            kind: node.kind().to_string(),
            relation: relation.unwrap_or_else(|| "-".to_string()),
            signature,
            location,
        }
    }
}

pub fn shorten_fqn(fqn: &str) -> String {
    let parts: Vec<&str> = fqn.split('.').collect();
    if parts.len() <= 2 {
        return fqn.to_string();
    }
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i < parts.len() - 2 {
            if let Some(c) = part.chars().next() {
                result.push(c);
                result.push('.');
            }
        } else {
            result.push_str(part);
            if i < parts.len() - 1 {
                result.push('.');
            }
        }
    }
    result
}

fn fmt_type(t: &TypeRef) -> String {
    match t {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(s) => s.split('.').last().unwrap_or(s).to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args.iter().map(fmt_type).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", fmt_type(base), args_str)
        },
        TypeRef::Array { element, dimensions } => {
            format!("{}{}", fmt_type(element), "[]".repeat(*dimensions))
        },
        _ => "?".to_string(),
    }
}

fn fmt_shell_signature(params: &[JavaParameter], return_type: &TypeRef) -> String {
    let return_type_str = fmt_type(return_type);
    let params_str = params.iter()
        .map(|p| fmt_type(&p.type_ref))
        .collect::<Vec<_>>()
        .join(", ");
    
    let total_len = params_str.len() + return_type_str.len();
    if total_len <= 50 {
        format!("({}) -> {}", params_str, return_type_str)
    } else {
        format!("(...)\n  -> {}", return_type_str)
    }
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
