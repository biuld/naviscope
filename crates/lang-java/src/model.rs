use naviscope_api::models::TypeRef;
use naviscope_core::engine::storage::model::StorageContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JavaElement {
    Class(JavaClass),
    Interface(JavaInterface),
    Enum(JavaEnum),
    Annotation(JavaAnnotation),
    Method(JavaMethod),
    Field(JavaField),
    Package(JavaPackage),
}

/// Optimized storage version of JavaElement
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JavaStorageElement {
    Class(JavaClassStorage),
    Interface(JavaInterfaceStorage),
    Enum(JavaEnumStorage),
    Annotation(JavaAnnotationStorage),
    Method(JavaMethodStorage),
    Field(JavaFieldStorage),
    Package(JavaPackageStorage),
}

impl JavaElement {
    pub fn to_storage(&self, ctx: &mut dyn StorageContext) -> JavaStorageElement {
        match self {
            JavaElement::Class(e) => JavaStorageElement::Class(JavaClassStorage {
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            }),
            JavaElement::Interface(e) => JavaStorageElement::Interface(JavaInterfaceStorage {
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            }),
            JavaElement::Enum(e) => JavaStorageElement::Enum(JavaEnumStorage {
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
                constants_sids: e.constants.iter().map(|s| ctx.intern_str(s)).collect(),
            }),
            JavaElement::Annotation(e) => JavaStorageElement::Annotation(JavaAnnotationStorage {
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            }),
            JavaElement::Method(e) => JavaStorageElement::Method(JavaMethodStorage {
                return_type: e.return_type.clone(),
                parameters: e
                    .parameters
                    .iter()
                    .map(|p| JavaParameterStorage {
                        name_sid: ctx.intern_str(&p.name),
                        type_ref: p.type_ref.clone(),
                    })
                    .collect(),
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
                is_constructor: e.is_constructor,
            }),
            JavaElement::Field(e) => JavaStorageElement::Field(JavaFieldStorage {
                type_ref: e.type_ref.clone(),
                modifiers_sids: e.modifiers.iter().map(|s| ctx.intern_str(s)).collect(),
            }),
            JavaElement::Package(_) => JavaStorageElement::Package(JavaPackageStorage {}),
        }
    }
}

impl JavaStorageElement {
    pub fn from_storage(&self, ctx: &dyn StorageContext) -> JavaElement {
        match self {
            JavaStorageElement::Class(e) => JavaElement::Class(JavaClass {
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
            }),
            JavaStorageElement::Interface(e) => JavaElement::Interface(JavaInterface {
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
            }),
            JavaStorageElement::Enum(e) => JavaElement::Enum(JavaEnum {
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
                constants: e
                    .constants_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
            }),
            JavaStorageElement::Annotation(e) => JavaElement::Annotation(JavaAnnotation {
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
            }),
            JavaStorageElement::Method(e) => JavaElement::Method(JavaMethod {
                return_type: e.return_type.clone(),
                parameters: e
                    .parameters
                    .iter()
                    .map(|p| JavaParameter {
                        name: ctx.resolve_str(p.name_sid).to_string(),
                        type_ref: p.type_ref.clone(),
                    })
                    .collect(),
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
                is_constructor: e.is_constructor,
            }),
            JavaStorageElement::Field(e) => JavaElement::Field(JavaField {
                type_ref: e.type_ref.clone(),
                modifiers: e
                    .modifiers_sids
                    .iter()
                    .map(|&sid| ctx.resolve_str(sid).to_string())
                    .collect(),
            }),
            JavaStorageElement::Package(_) => JavaElement::Package(JavaPackage {}),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaClass {
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaClassStorage {
    pub modifiers_sids: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaInterface {
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaInterfaceStorage {
    pub modifiers_sids: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaEnum {
    pub modifiers: Vec<String>,
    pub constants: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaEnumStorage {
    pub modifiers_sids: Vec<u32>,
    pub constants_sids: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaAnnotation {
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaAnnotationStorage {
    pub modifiers_sids: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaField {
    pub type_ref: TypeRef,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaFieldStorage {
    pub type_ref: TypeRef,
    pub modifiers_sids: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaMethod {
    pub return_type: TypeRef,
    pub parameters: Vec<JavaParameter>,
    pub modifiers: Vec<String>,
    pub is_constructor: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaMethodStorage {
    pub return_type: TypeRef,
    pub parameters: Vec<JavaParameterStorage>,
    pub modifiers_sids: Vec<u32>,
    pub is_constructor: bool,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaPackage {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaPackageStorage {}
