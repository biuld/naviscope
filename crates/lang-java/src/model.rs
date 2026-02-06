use naviscope_api::models::{NodeMetadata, TypeRef};
use naviscope_plugin::{IndexMetadata, SymbolInterner};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// Uninterned metadata used during parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JavaIndexMetadata {
    Class {
        modifiers: Vec<String>,
    },
    Interface {
        modifiers: Vec<String>,
    },
    Enum {
        modifiers: Vec<String>,
        constants: Vec<String>,
    },
    Annotation {
        modifiers: Vec<String>,
    },
    Method {
        modifiers: Vec<String>,
        return_type: TypeRef,
        parameters: Vec<JavaParameter>,
        is_constructor: bool,
    },
    Field {
        modifiers: Vec<String>,
        type_ref: TypeRef,
    },
    Package,
}

impl IndexMetadata for JavaIndexMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn intern(&self, interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata> {
        Arc::new(self.to_storage(interner))
    }

    fn to_cached_metadata(&self) -> naviscope_plugin::CachedMetadata {
        self.to_cached_metadata()
    }
}

/// Interned metadata stored in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JavaNodeMetadata {
    Class {
        modifiers_sids: Vec<u32>,
    },
    Interface {
        modifiers_sids: Vec<u32>,
    },
    Enum {
        modifiers_sids: Vec<u32>,
        constants_sids: Vec<u32>,
    },
    Annotation {
        modifiers_sids: Vec<u32>,
    },
    Method {
        modifiers_sids: Vec<u32>,
        return_type: TypeRef,
        parameters: Vec<JavaParameterStorage>,
        is_constructor: bool,
    },
    Field {
        modifiers_sids: Vec<u32>,
        type_ref: TypeRef,
    },
    Package,
}

impl JavaIndexMetadata {
    pub fn deserialize_for_cache(_version: u32, bytes: &[u8]) -> Arc<dyn IndexMetadata> {
        // In the future, we can switch on version here to handle migrations
        match rmp_serde::from_slice::<Self>(bytes) {
            Ok(meta) => Arc::new(meta),
            Err(_) => Arc::new(naviscope_api::models::graph::EmptyMetadata),
        }
    }

    pub fn to_cached_metadata(&self) -> naviscope_plugin::CachedMetadata {
        naviscope_plugin::CachedMetadata {
            type_tag: "java".to_string(),
            version: 1, // Current version
            data: rmp_serde::to_vec(self).unwrap_or_default(),
        }
    }

    pub fn to_storage(&self, ctx: &mut dyn SymbolInterner) -> JavaNodeMetadata {
        match self {
            JavaIndexMetadata::Class { modifiers } => JavaNodeMetadata::Class {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            },
            JavaIndexMetadata::Interface { modifiers } => JavaNodeMetadata::Interface {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            },
            JavaIndexMetadata::Enum {
                modifiers,
                constants,
            } => JavaNodeMetadata::Enum {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
                constants_sids: constants.iter().map(|s| ctx.intern_str(s)).collect(),
            },
            JavaIndexMetadata::Annotation { modifiers } => JavaNodeMetadata::Annotation {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            },
            JavaIndexMetadata::Method {
                modifiers,
                return_type,
                parameters,
                is_constructor,
            } => JavaNodeMetadata::Method {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
                return_type: return_type.clone(),
                parameters: parameters
                    .iter()
                    .map(|p| JavaParameterStorage {
                        name_sid: ctx.intern_str(&p.name),
                        type_ref: p.type_ref.clone(),
                    })
                    .collect(),
                is_constructor: *is_constructor,
            },
            JavaIndexMetadata::Field {
                modifiers,
                type_ref,
            } => JavaNodeMetadata::Field {
                modifiers_sids: modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
                type_ref: type_ref.clone(),
            },
            JavaIndexMetadata::Package => JavaNodeMetadata::Package,
        }
    }
}

impl NodeMetadata for JavaNodeMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaParameter {
    pub name: String,
    pub type_ref: TypeRef,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaParameterStorage {
    pub name_sid: u32,
    pub type_ref: TypeRef,
}

pub fn fmt_type(t: &TypeRef) -> String {
    match t {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(s) => s.split('.').last().unwrap_or(s).to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(|a| fmt_type(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", fmt_type(base), args_str)
        }
        TypeRef::Array {
            element,
            dimensions,
        } => {
            format!("{}{}", fmt_type(element), "[]".repeat(*dimensions))
        }
        _ => "?".to_string(),
    }
}

pub fn fmt_type_uninterned(t: &TypeRef) -> String {
    match t {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(s) => s.split('.').last().unwrap_or(s).to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(|a| fmt_type_uninterned(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", fmt_type_uninterned(base), args_str)
        }
        TypeRef::Array {
            element,
            dimensions,
        } => {
            format!(
                "{}{}",
                fmt_type_uninterned(element),
                "[]".repeat(*dimensions)
            )
        }
        _ => "?".to_string(),
    }
}
