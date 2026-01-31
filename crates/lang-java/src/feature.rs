use crate::model::JavaElement;
use naviscope_core::model::graph::GraphNode;
use naviscope_core::model::signature::TypeRef;
use naviscope_core::plugin::LanguageFeatureProvider;

pub struct JavaFeatureProvider;

impl JavaFeatureProvider {
    pub fn new() -> Self {
        Self
    }

    fn fmt_type(&self, t: &TypeRef) -> String {
        match t {
            TypeRef::Raw(s) => s.clone(),
            TypeRef::Id(s) => s.split('.').last().unwrap_or(s).to_string(),
            TypeRef::Generic { base, args } => {
                let args_str = args
                    .iter()
                    .map(|a| self.fmt_type(a))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", self.fmt_type(base), args_str)
            }
            TypeRef::Array {
                element,
                dimensions,
            } => {
                format!("{}{}", self.fmt_type(element), "[]".repeat(*dimensions))
            }
            _ => "?".to_string(),
        }
    }
}

impl LanguageFeatureProvider for JavaFeatureProvider {
    fn detail_view(&self, node: &GraphNode) -> Option<String> {
        if node.lang != "java" {
            return None;
        }

        let element = serde_json::from_value::<JavaElement>(node.metadata.clone()).ok()?;

        match element {
            JavaElement::Class(c) => {
                let mut detail = format!("**class** {}", c.name);
                if !c.modifiers.is_empty() {
                    detail = format!("{} {}", c.modifiers.join(" "), detail);
                }
                Some(detail)
            }
            JavaElement::Interface(i) => {
                let mut detail = format!("**interface** {}", i.name);
                if !i.modifiers.is_empty() {
                    detail = format!("{} {}", i.modifiers.join(" "), detail);
                }
                Some(detail)
            }
            JavaElement::Method(m) => {
                let params_str = m
                    .parameters
                    .iter()
                    .map(|p| format!("{}: {}", p.name, self.fmt_type(&p.type_ref)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type_str = self.fmt_type(&m.return_type);
                let mut detail = format!("**{}**({}) -> {}", m.name, params_str, return_type_str);
                if !m.modifiers.is_empty() {
                    detail = format!("{} {}", m.modifiers.join(" "), detail);
                }
                Some(detail)
            }
            JavaElement::Field(f) => {
                let mut detail = format!("**{}**: {}", f.name, self.fmt_type(&f.type_ref));
                if !f.modifiers.is_empty() {
                    detail = format!("{} {}", f.modifiers.join(" "), detail);
                }
                Some(detail)
            }
            _ => None,
        }
    }

    fn signature(&self, node: &GraphNode) -> Option<String> {
        if node.lang != "java" {
            return None;
        }

        let element = serde_json::from_value::<JavaElement>(node.metadata.clone()).ok()?;

        match element {
            JavaElement::Method(m) => {
                let params_str = m
                    .parameters
                    .iter()
                    .map(|p| self.fmt_type(&p.type_ref))
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type_str = self.fmt_type(&m.return_type);
                Some(format!("({}) -> {}", params_str, return_type_str))
            }
            JavaElement::Field(f) => Some(format!("{} {}", self.fmt_type(&f.type_ref), f.name)),
            _ => None,
        }
    }

    fn modifiers(&self, node: &GraphNode) -> Vec<String> {
        if node.lang != "java" {
            return vec![];
        }

        let element = serde_json::from_value::<JavaElement>(node.metadata.clone());
        if let Ok(element) = element {
            match element {
                JavaElement::Class(c) => c.modifiers,
                JavaElement::Interface(i) => i.modifiers,
                JavaElement::Method(m) => m.modifiers,
                JavaElement::Field(f) => f.modifiers,
                JavaElement::Enum(e) => e.modifiers,
                JavaElement::Annotation(a) => a.modifiers,
                _ => vec![],
            }
        } else {
            vec![]
        }
    }
}
