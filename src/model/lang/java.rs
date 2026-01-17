use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub entities: Vec<JavaElement>,
}

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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaClass {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub superclass: Option<String>,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaInterface {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub extends: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaEnum {
    pub name: String,
    pub id: String, // FQN
    pub modifiers: Vec<String>,
    pub interfaces: Vec<String>,
    pub constants: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaAnnotation {
    pub name: String,
    pub id: String,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaField {
    pub name: String,
    pub id: String,
    pub type_name: String,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaMethod {
    pub name: String,
    pub id: String,
    pub return_type: String,
    pub parameters: Vec<JavaParameter>,
    pub modifiers: Vec<String>,
    pub is_constructor: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaParameter {
    pub name: String,
    pub type_name: String,
}
