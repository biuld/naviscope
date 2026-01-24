use crate::model::graph::Range;
use crate::model::signature::TypeRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum JavaElement {
    Class(JavaClass),
    Interface(JavaInterface),
    Enum(JavaEnum),
    Annotation(JavaAnnotation),
    Method(JavaMethod),
    Field(JavaField),
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
        }
    }

    pub fn range(&self) -> Option<&Range> {
        match self {
            JavaElement::Class(e) => e.range.as_ref(),
            JavaElement::Interface(e) => e.range.as_ref(),
            JavaElement::Enum(e) => e.range.as_ref(),
            JavaElement::Annotation(e) => e.range.as_ref(),
            JavaElement::Method(e) => e.range.as_ref(),
            JavaElement::Field(e) => e.range.as_ref(),
        }
    }

    pub fn name_range(&self) -> Option<&Range> {
        match self {
            JavaElement::Class(e) => e.name_range.as_ref(),
            JavaElement::Interface(e) => e.name_range.as_ref(),
            JavaElement::Enum(e) => e.name_range.as_ref(),
            JavaElement::Annotation(e) => e.name_range.as_ref(),
            JavaElement::Method(e) => e.name_range.as_ref(),
            JavaElement::Field(e) => e.name_range.as_ref(),
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
