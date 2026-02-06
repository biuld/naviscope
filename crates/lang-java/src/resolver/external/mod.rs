use naviscope_plugin::{ExternalResolver, GlobalParseResult, IndexNode};
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use zip::ZipArchive;

mod converter;
use converter::{JavaModifierConverter, JavaTypeConverter};

pub struct JavaExternalResolver;

impl ExternalResolver for JavaExternalResolver {
    fn index_asset(
        &self,
        asset: &Path,
    ) -> std::result::Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let file = File::open(asset)?;
        let mut archive = ZipArchive::new(file)?;
        let mut packages = HashSet::new();

        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name();

            if name.ends_with(".class") && !name.contains('$') {
                if let Some(slash_idx) = name.rfind('/') {
                    let package = name[..slash_idx].replace('/', ".");
                    if !package.starts_with("META-INF") {
                        packages.insert(package);
                    }
                }
            }
        }

        let mut result: Vec<String> = packages.into_iter().collect();
        result.sort();
        Ok(result)
    }

    fn generate_stub(
        &self,
        fqn: &str,
        asset: &Path,
    ) -> std::result::Result<IndexNode, Box<dyn std::error::Error + Send + Sync>> {
        let file = File::open(asset)?;
        let mut archive = ZipArchive::new(file)?;

        let mut current_fqn = fqn.to_string();
        let mut member_parts = Vec::new();

        let mut entry = loop {
            let class_path = current_fqn.replace('.', "/") + ".class";
            if let Ok(e) = archive.by_name(&class_path) {
                break e;
            }

            let inner_path = current_fqn.replace('.', "/");
            if let Some(idx) = inner_path.rfind('/') {
                let mut try_inner = inner_path.clone();
                try_inner.replace_range(idx..idx + 1, "$");
                if let Ok(e) = archive.by_name(&(try_inner + ".class")) {
                    break e;
                }
            }

            if let Some(idx) = current_fqn.rfind('.') {
                member_parts.push(current_fqn[idx + 1..].to_string());
                current_fqn = current_fqn[..idx].to_string();
            } else {
                return Err(
                    format!("Could not find class for {} in {}", fqn, asset.display()).into(),
                );
            }
        };

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let class =
            cafebabe::parse_class(&bytes).map_err(|e| format!("Failed to parse class: {:?}", e))?;

        if member_parts.is_empty() {
            let name = fqn.split('.').last().unwrap_or(fqn).to_string();
            let kind = if class
                .access_flags
                .contains(cafebabe::ClassAccessFlags::INTERFACE)
            {
                naviscope_api::models::graph::NodeKind::Interface
            } else if class
                .access_flags
                .contains(cafebabe::ClassAccessFlags::ANNOTATION)
            {
                naviscope_api::models::graph::NodeKind::Annotation
            } else if class
                .access_flags
                .contains(cafebabe::ClassAccessFlags::ENUM)
            {
                naviscope_api::models::graph::NodeKind::Enum
            } else {
                naviscope_api::models::graph::NodeKind::Class
            };

            let modifiers = JavaModifierConverter::parse_class(class.access_flags);
            let metadata = crate::model::JavaIndexMetadata::Class { modifiers };

            return Ok(IndexNode {
                id: naviscope_api::models::symbol::NodeId::Flat(fqn.to_string()),
                name,
                kind,
                lang: "java".to_string(),
                source: naviscope_api::models::graph::NodeSource::External,
                status: naviscope_api::models::graph::ResolutionStatus::Stubbed,
                location: None,
                metadata: Arc::new(metadata),
            });
        }

        member_parts.reverse();
        let member_name = member_parts.join(".");

        for field in &class.fields {
            if field.name == member_name {
                let type_ref = JavaTypeConverter::convert_field(&field.descriptor);
                let modifiers = JavaModifierConverter::parse_field(field.access_flags);
                let metadata = crate::model::JavaIndexMetadata::Field {
                    modifiers,
                    type_ref,
                };
                return Ok(IndexNode {
                    id: naviscope_api::models::symbol::NodeId::Flat(fqn.to_string()),
                    name: member_name.clone(),
                    kind: naviscope_api::models::graph::NodeKind::Field,
                    lang: "java".to_string(),
                    source: naviscope_api::models::graph::NodeSource::External,
                    status: naviscope_api::models::graph::ResolutionStatus::Stubbed,
                    location: None,
                    metadata: Arc::new(metadata),
                });
            }
        }

        for method in &class.methods {
            if method.name == member_name {
                let (return_type, parameters) =
                    JavaTypeConverter::convert_method(&method.descriptor);
                let modifiers = JavaModifierConverter::parse_method(method.access_flags);
                let metadata = crate::model::JavaIndexMetadata::Method {
                    modifiers,
                    return_type,
                    parameters,
                    is_constructor: member_name == "<init>",
                };
                return Ok(IndexNode {
                    id: naviscope_api::models::symbol::NodeId::Flat(fqn.to_string()),
                    name: if member_name == "<init>" {
                        fqn.split('.').nth_back(1).unwrap_or(fqn).to_string()
                    } else {
                        member_name.clone()
                    },
                    kind: if member_name == "<init>" {
                        naviscope_api::models::graph::NodeKind::Constructor
                    } else {
                        naviscope_api::models::graph::NodeKind::Method
                    },
                    lang: "java".to_string(),
                    source: naviscope_api::models::graph::NodeSource::External,
                    status: naviscope_api::models::graph::ResolutionStatus::Stubbed,
                    location: None,
                    metadata: Arc::new(metadata),
                });
            }
        }

        Err(format!("Member {} not found in class {}", member_name, current_fqn).into())
    }

    fn resolve_source(
        &self,
        _fqn: &str,
        _source_asset: &Path,
    ) -> std::result::Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("Source resolution not yet implemented".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_test_jar(path: &Path) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("com/example/Test.class", options).unwrap();
        // CAFEBABE header
        zip.write_all(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34])
            .unwrap();

        zip.finish().unwrap();
    }

    #[test]
    fn test_index_asset() {
        let dir = tempdir().unwrap();
        let jar_path = dir.path().join("test.jar");
        create_test_jar(&jar_path);

        let resolver = JavaExternalResolver;
        let packages = resolver.index_asset(&jar_path).unwrap();

        assert_eq!(packages, vec!["com.example".to_string()]);
    }
}
