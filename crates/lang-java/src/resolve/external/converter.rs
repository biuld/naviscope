use naviscope_api::models::TypeRef;
use ristretto_classfile::{BaseType, ClassAccessFlags, FieldAccessFlags, FieldType, MethodAccessFlags};

pub struct JavaTypeConverter;

impl JavaTypeConverter {
    pub fn convert_field(ty: &FieldType) -> TypeRef {
        Self::convert_type(ty)
    }

    pub fn convert_method(
        descriptor: &str,
        is_varargs: bool,
    ) -> Result<(TypeRef, Vec<crate::model::JavaParameter>), ristretto_classfile::Error> {
        let (params, ret) = FieldType::parse_method_descriptor(descriptor)?;
        let return_type = match ret {
            None => TypeRef::Raw("void".to_string()),
            Some(field_type) => Self::convert_field(&field_type),
        };

        let parameters = params
            .iter()
            .enumerate()
            .map(|(i, field_type)| crate::model::JavaParameter {
                name: format!("arg{}", i),
                type_ref: Self::convert_field(field_type),
                is_varargs: is_varargs && i == params.len().saturating_sub(1),
            })
            .collect();

        Ok((return_type, parameters))
    }

    pub fn convert_type(ty: &FieldType) -> TypeRef {
        match ty {
            FieldType::Base(BaseType::Byte) => TypeRef::Raw("byte".to_string()),
            FieldType::Base(BaseType::Char) => TypeRef::Raw("char".to_string()),
            FieldType::Base(BaseType::Double) => TypeRef::Raw("double".to_string()),
            FieldType::Base(BaseType::Float) => TypeRef::Raw("float".to_string()),
            FieldType::Base(BaseType::Int) => TypeRef::Raw("int".to_string()),
            FieldType::Base(BaseType::Long) => TypeRef::Raw("long".to_string()),
            FieldType::Base(BaseType::Short) => TypeRef::Raw("short".to_string()),
            FieldType::Base(BaseType::Boolean) => TypeRef::Raw("boolean".to_string()),
            FieldType::Object(name) => TypeRef::Id(name.replace('/', ".")),
            FieldType::Array(component) => {
                let mut dimensions = 1usize;
                let mut current = component.as_ref();
                while let FieldType::Array(inner) = current {
                    dimensions += 1;
                    current = inner.as_ref();
                }

                TypeRef::Array {
                    element: Box::new(Self::convert_type(current)),
                    dimensions,
                }
            }
        }
    }
}

pub struct JavaModifierConverter;

impl JavaModifierConverter {
    pub fn parse_class(flags: ClassAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(ClassAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(ClassAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(ClassAccessFlags::ABSTRACT) && !flags.contains(ClassAccessFlags::INTERFACE)
        {
            mods.push("abstract".into());
        }
        mods
    }

    pub fn parse_field(flags: FieldAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(FieldAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(FieldAccessFlags::PRIVATE) {
            mods.push("private".into());
        }
        if flags.contains(FieldAccessFlags::PROTECTED) {
            mods.push("protected".into());
        }
        if flags.contains(FieldAccessFlags::STATIC) {
            mods.push("static".into());
        }
        if flags.contains(FieldAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(FieldAccessFlags::VOLATILE) {
            mods.push("volatile".into());
        }
        if flags.contains(FieldAccessFlags::TRANSIENT) {
            mods.push("transient".into());
        }
        mods
    }

    pub fn parse_method(flags: MethodAccessFlags) -> Vec<String> {
        let mut mods = Vec::new();
        if flags.contains(MethodAccessFlags::PUBLIC) {
            mods.push("public".into());
        }
        if flags.contains(MethodAccessFlags::PRIVATE) {
            mods.push("private".into());
        }
        if flags.contains(MethodAccessFlags::PROTECTED) {
            mods.push("protected".into());
        }
        if flags.contains(MethodAccessFlags::STATIC) {
            mods.push("static".into());
        }
        if flags.contains(MethodAccessFlags::FINAL) {
            mods.push("final".into());
        }
        if flags.contains(MethodAccessFlags::SYNCHRONIZED) {
            mods.push("synchronized".into());
        }
        if flags.contains(MethodAccessFlags::NATIVE) {
            mods.push("native".into());
        }
        if flags.contains(MethodAccessFlags::ABSTRACT) {
            mods.push("abstract".into());
        }
        if flags.contains(MethodAccessFlags::STRICT) {
            mods.push("strictfp".into());
        }
        mods
    }
}
