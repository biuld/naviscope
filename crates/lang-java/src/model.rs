use naviscope_core::model::graph::Range;
use naviscope_core::model::signature::TypeRef;
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

impl JavaElement {
    pub fn id(&self) -> &str {
        match self {
            JavaElement::Class(e) => &e.id,
            JavaElement::Interface(e) => &e.id,
            JavaElement::Enum(e) => &e.id,
            JavaElement::Annotation(e) => &e.id,
            JavaElement::Method(e) => &e.id,
            JavaElement::Field(e) => &e.id,
            JavaElement::Package(e) => &e.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            JavaElement::Class(e) => &e.name,
            JavaElement::Interface(e) => &e.name,
            JavaElement::Enum(e) => &e.name,
            JavaElement::Annotation(e) => &e.name,
            JavaElement::Method(e) => &e.name,
            JavaElement::Field(e) => &e.name,
            JavaElement::Package(e) => &e.name,
        }
    }

    pub fn range(&self) -> Option<Range> {
        match self {
            JavaElement::Class(e) => e.range,
            JavaElement::Interface(e) => e.range,
            JavaElement::Enum(e) => e.range,
            JavaElement::Annotation(e) => e.range,
            JavaElement::Method(e) => e.range,
            JavaElement::Field(e) => e.range,
            JavaElement::Package(_) => None,
        }
    }

    pub fn name_range(&self) -> Option<Range> {
        match self {
            JavaElement::Class(e) => e.name_range,
            JavaElement::Interface(e) => e.name_range,
            JavaElement::Enum(e) => e.name_range,
            JavaElement::Annotation(e) => e.name_range,
            JavaElement::Method(e) => e.name_range,
            JavaElement::Field(e) => e.name_range,
            JavaElement::Package(_) => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaClass {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaInterface {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaEnum {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub constants: Vec<String>,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaAnnotation {
    pub name: String,
    pub id: String,
    pub modifiers: Vec<String>,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaField {
    pub name: String,
    pub id: String,
    pub type_ref: TypeRef,
    pub modifiers: Vec<String>,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaMethod {
    pub name: String,
    pub id: String,
    pub return_type: TypeRef,
    pub parameters: Vec<JavaParameter>,
    pub modifiers: Vec<String>,
    pub is_constructor: bool,
    pub range: Option<Range>,
    pub name_range: Option<Range>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaParameter {
    pub name: String,
    pub type_ref: TypeRef,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaPackage {
    pub name: String,
    pub id: String,
}
