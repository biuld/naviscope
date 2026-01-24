use crate::model::graph::{BuildElement, CodeElement, GraphNode};
use crate::model::signature::TypeRef;
use serde::Serialize;

fn fmt_type(t: &TypeRef) -> String {
    match t {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(s) => s.clone(),
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

/// Format method signature with smart line breaks for long signatures
fn fmt_method_signature(params: &[crate::model::lang::java::JavaParameter], return_type: &TypeRef) -> String {
    let return_type_str = fmt_type(return_type);
    
    // Format parameters (only types, no names)
    let params_str = if params.is_empty() {
        String::new()
    } else {
        params
            .iter()
            .map(|p| fmt_type(&p.type_ref))
            .collect::<Vec<_>>()
            .join(", ")
    };
    
    // Estimate total length
    let total_len = params_str.len() + return_type_str.len() + 10; // +10 for "() -> "
    
    // If signature is short, keep it on one line
    if total_len <= 80 {
        return format!("({}) -> {}", params_str, return_type_str);
    }
    
    // For long signatures, use multi-line format with proper alignment
    if params.is_empty() {
        // No params, just break before return type
        format!("()\n  -> {}", return_type_str)
    } else if params.len() > 3 || params_str.len() > 50 {
        // Many params or long param list: each param on its own line
        let param_lines: Vec<String> = params
            .iter()
            .map(|p| format!("  {}", fmt_type(&p.type_ref)))
            .collect();
        format!("(\n{}\n) -> {}", param_lines.join(",\n"), return_type_str)
    } else {
        // Few params but long return type: break before return type
        format!("({})\n  -> {}", params_str, return_type_str)
    }
}

/// Summary information of a node, intended to provide a concise context for the Agent
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
                        if m.is_constructor {
                            let params_str = if m.parameters.is_empty() {
                                String::new()
                            } else {
                                m.parameters
                                    .iter()
                                    .map(|p| format!("{} {}", fmt_type(&p.type_ref), p.name))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            };
                            Some(format!("{}({})", m.name, params_str))
                        } else {
                            Some(fmt_method_signature(&m.parameters, &m.return_type))
                        }
                    }
                    crate::model::lang::java::JavaElement::Field(f) => {
                        Some(format!("{} {}", fmt_type(&f.type_ref), f.name))
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
            fqn: node.fqn().to_string(),
            name: node.name().to_string(),
            kind: node.kind().to_string(),
            signature,
            location,
        }
    }
}
