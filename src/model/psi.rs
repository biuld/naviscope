use serde::{Deserialize, Serialize};

// --- Language-neutral PSI Traits (to be expanded) ---

pub trait CodeElement {
    // a unique identifier for this element
    fn id(&self) -> &str;
}

pub trait NamedElement: CodeElement {
    fn name(&self) -> &str;
}

// --- Java-specific PSI structs ---

#[derive(Serialize, Deserialize, Debug)]
pub enum JavaElement {
    Class(JavaClass),
    Method(JavaMethod),
    Field(JavaField),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JavaClass {
    pub id: String,
    pub name: String,
    // ... other fields like visibility, modifiers, etc.
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JavaMethod {
    pub id: String,
    pub name: String,
    // ... other fields like parameters, return type, etc.
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JavaField {
    pub id: String,
    pub name: String,
    // ... other fields like type, visibility, etc.
}
