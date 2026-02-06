use cafebabe::descriptors::{FieldDescriptor, FieldType, MethodDescriptor, ReturnDescriptor};
use naviscope_api::models::TypeRef;

pub struct JavaTypeConverter;

impl JavaTypeConverter {
    pub fn convert_field(desc: &FieldDescriptor) -> TypeRef {
        let mut tr = Self::convert_type(&desc.field_type);
        if desc.dimensions > 0 {
            tr = TypeRef::Array {
                element: Box::new(tr),
                dimensions: desc.dimensions as usize,
            };
        }
        tr
    }

    pub fn convert_method(desc: &MethodDescriptor) -> (TypeRef, Vec<crate::model::JavaParameter>) {
        let return_type = match &desc.return_type {
            ReturnDescriptor::Void => TypeRef::Raw("void".to_string()),
            ReturnDescriptor::Return(field_desc) => Self::convert_field(field_desc),
        };

        let parameters = desc
            .parameters
            .iter()
            .enumerate()
            .map(|(i, field_desc)| crate::model::JavaParameter {
                name: format!("arg{}", i),
                type_ref: Self::convert_field(field_desc),
            })
            .collect();

        (return_type, parameters)
    }

    pub fn convert_type(ty: &FieldType) -> TypeRef {
        match ty {
            FieldType::Byte => TypeRef::Raw("byte".to_string()),
            FieldType::Char => TypeRef::Raw("char".to_string()),
            FieldType::Double => TypeRef::Raw("double".to_string()),
            FieldType::Float => TypeRef::Raw("float".to_string()),
            FieldType::Integer => TypeRef::Raw("int".to_string()),
            FieldType::Long => TypeRef::Raw("long".to_string()),
            FieldType::Short => TypeRef::Raw("short".to_string()),
            FieldType::Boolean => TypeRef::Raw("boolean".to_string()),
            FieldType::Object(name) => TypeRef::Id(name.replace('/', ".")),
        }
    }
}

pub struct JavaModifierConverter;

impl JavaModifierConverter {
    pub fn parse_class(flags: cafebabe::ClassAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(cafebabe::ClassAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(cafebabe::ClassAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(cafebabe::ClassAccessFlags::ABSTRACT)
            && !flags.contains(cafebabe::ClassAccessFlags::INTERFACE)
        {
            mods.push("abstract".into());
        }
        mods
    }

    pub fn parse_field(flags: cafebabe::FieldAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(cafebabe::FieldAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::PRIVATE) {
            mods.push("private".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::PROTECTED) {
            mods.push("protected".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::STATIC) {
            mods.push("static".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::VOLATILE) {
            mods.push("volatile".into());
        }
        if flags.contains(cafebabe::FieldAccessFlags::TRANSIENT) {
            mods.push("transient".into());
        }
        mods
    }

    pub fn parse_method(flags: cafebabe::MethodAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(cafebabe::MethodAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::PRIVATE) {
            mods.push("private".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::PROTECTED) {
            mods.push("protected".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::STATIC) {
            mods.push("static".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::SYNCHRONIZED) {
            mods.push("synchronized".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::NATIVE) {
            mods.push("native".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::ABSTRACT) {
            mods.push("abstract".into());
        }
        if flags.contains(cafebabe::MethodAccessFlags::STRICT) {
            mods.push("strictfp".into());
        }
        mods
    }
}
