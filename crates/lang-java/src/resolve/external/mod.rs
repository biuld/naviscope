use naviscope_plugin::{
    AssetEntry, AssetIndexer, AssetSource, AssetSourceLocator, GlobalParseResult, IndexNode,
    StubGenerator,
};
use ristretto_classfile::{ClassAccessFlags, ClassFile, MethodAccessFlags};
use ristretto_jimage::Image;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::ZipArchive;

mod converter;
use converter::{JavaModifierConverter, JavaTypeConverter};

pub struct JavaExternalResolver;

impl JavaExternalResolver {
    fn extract_packages_from_zip(
        archive: &mut ZipArchive<File>,
    ) -> std::result::Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
        let mut packages = HashSet::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name();

            if name.ends_with(".class") && !name.contains('$') {
                if let Some(slash_idx) = name.rfind('/') {
                    let package = name[..slash_idx].replace('/', ".");
                    if !package.starts_with("META-INF") {
                        packages.insert(package);
                    }
                }
            }
        }
        Ok(packages)
    }

    fn extract_packages_from_jimage(image: &Image) -> HashSet<String> {
        let mut packages = HashSet::new();
        for resource_result in image.iter() {
            if let Ok(resource) = resource_result {
                if resource.extension() == "class" && !resource.base().contains('$') {
                    let parent = resource.parent();
                    let path_without_module = if parent.starts_with('/') {
                        let s = &parent[1..];
                        if let Some(idx) = s.find('/') {
                            &s[idx + 1..]
                        } else {
                            s
                        }
                    } else {
                        &parent
                    };

                    let package = path_without_module.replace('/', ".");
                    if !package.is_empty() {
                        packages.insert(package);
                    }
                }
            }
        }
        packages
    }
}

impl JavaExternalResolver {
    pub fn index_asset(
        &self,
        asset: &Path,
    ) -> std::result::Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        // Detect format via magic bytes
        let mut file = File::open(asset)?;
        let mut magic = [0u8; 4];
        if std::io::Read::read(&mut file, &mut magic).is_err() {
            return Ok(vec![]);
        }

        let packages: HashSet<String> = match &magic {
            // ZIP magic: PK\x03\x04 or PK\x05\x06 (empty) or PK\x07\x08 (spanned)
            [0x50, 0x4B, _, _] => {
                // Reset file position and parse as ZIP
                std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(0))?;
                let mut archive = ZipArchive::new(file)?;
                Self::extract_packages_from_zip(&mut archive)?
            }
            // JImage magic: CAFEDADA (big-endian) or DADAFECA (little-endian)
            [0xCA, 0xFE, 0xDA, 0xDA] | [0xDA, 0xDA, 0xFE, 0xCA] => {
                drop(file); // Close file handle before reopening via ristretto_jimage
                let image = Image::from_file(asset)?;
                Self::extract_packages_from_jimage(&image)
            }
            _ => {
                // Unknown format, skip silently
                return Ok(vec![]);
            }
        };

        let mut result: Vec<String> = packages.into_iter().collect();
        result.sort();
        Ok(result)
    }

    pub fn generate_stub(
        &self,
        fqn: &str,
        asset: &Path,
    ) -> std::result::Result<IndexNode, Box<dyn std::error::Error + Send + Sync>> {
        let file = File::open(asset)?;
        let mut current_fqn = fqn.to_string();
        let mut member_parts = Vec::new();

        let bytes = match ZipArchive::new(file) {
            Ok(mut archive) => {
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
                        return Err(format!(
                            "Could not find class for {} in {}",
                            fqn,
                            asset.display()
                        )
                        .into());
                    }
                };

                let mut b = Vec::new();
                entry.read_to_end(&mut b)?;
                b
            }
            Err(_) => {
                // Try JImage
                let image = Image::from_file(asset)?;
                let mut bytes: Option<Vec<u8>> = None;

                loop {
                    let class_path = current_fqn.replace('.', "/") + ".class";
                    // Since we don't know the module, we search all modules
                    for resource_result in image.iter() {
                        if let Ok(resource) = resource_result {
                            let name = resource.name();
                            if name == class_path || name.ends_with(&format!("/{}", class_path)) {
                                bytes = Some(resource.data().to_vec());
                                break;
                            }
                        }
                    }

                    if bytes.is_some() {
                        break;
                    }

                    // Try inner class
                    let inner_path = current_fqn.replace('.', "/");
                    if let Some(idx) = inner_path.rfind('/') {
                        let mut try_inner = inner_path.clone();
                        try_inner.replace_range(idx..idx + 1, "$");
                        let try_inner_path = try_inner + ".class";

                        for resource_result in image.iter() {
                            if let Ok(resource) = resource_result {
                                let name = resource.name();
                                if name == try_inner_path
                                    || name.ends_with(&format!("/{}", try_inner_path))
                                {
                                    bytes = Some(resource.data().to_vec());
                                    break;
                                }
                            }
                        }
                    }

                    if bytes.is_some() {
                        break;
                    }

                    if let Some(idx) = current_fqn.rfind('.') {
                        member_parts.push(current_fqn[idx + 1..].to_string());
                        current_fqn = current_fqn[..idx].to_string();
                    } else {
                        return Err(format!(
                            "Could not find class for {} in jimage {}",
                            fqn,
                            asset.display()
                        )
                        .into());
                    }
                }

                bytes.ok_or_else(|| format!("Class {} not found in jimage", fqn))?
            }
        };

        let class = ClassFile::from_bytes(&mut Cursor::new(bytes))
            .map_err(|e| format!("Failed to parse class: {e:?}"))?;

        if member_parts.is_empty() {
            let name = fqn.split('.').last().unwrap_or(fqn).to_string();
            let kind = if class
                .access_flags
                .contains(ClassAccessFlags::INTERFACE)
            {
                naviscope_api::models::graph::NodeKind::Interface
            } else if class
                .access_flags
                .contains(ClassAccessFlags::ANNOTATION)
            {
                naviscope_api::models::graph::NodeKind::Annotation
            } else if class
                .access_flags
                .contains(ClassAccessFlags::ENUM)
            {
                naviscope_api::models::graph::NodeKind::Enum
            } else {
                naviscope_api::models::graph::NodeKind::Class
            };

            let modifiers = JavaModifierConverter::parse_class(class.access_flags);
            let metadata = crate::model::JavaIndexMetadata::Class {
                modifiers,
                type_parameters: vec![],
            };

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
            let field_name = class
                .constant_pool
                .try_get_utf8(field.name_index)
                .map_err(|e| format!("Failed to parse field name: {e:?}"))?;

            if field_name == member_name {
                let type_ref = JavaTypeConverter::convert_field(&field.field_type);
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
            let method_name = class
                .constant_pool
                .try_get_utf8(method.name_index)
                .map_err(|e| format!("Failed to parse method name: {e:?}"))?;

            if method_name == member_name {
                let method_descriptor = class
                    .constant_pool
                    .try_get_utf8(method.descriptor_index)
                    .map_err(|e| format!("Failed to parse method descriptor: {e:?}"))?;
                let is_varargs = method.access_flags.contains(MethodAccessFlags::VARARGS);
                let (return_type, parameters) =
                    JavaTypeConverter::convert_method(method_descriptor, is_varargs)
                        .map_err(|e| format!("Failed to parse method signature: {e:?}"))?;
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

    pub fn resolve_source(
        &self,
        _fqn: &str,
        _source_asset: &Path,
    ) -> std::result::Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("Source resolution not yet implemented".into())
    }
}

impl AssetIndexer for JavaExternalResolver {
    fn can_index(&self, asset: &Path) -> bool {
        let ext = asset
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let file_name = asset.file_name().and_then(|n| n.to_str()).unwrap_or("");

        ext == "jar" || ext == "jmod" || file_name == "modules"
    }

    fn index(
        &self,
        asset: &Path,
    ) -> std::result::Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        self.index_asset(asset)
    }
}

impl StubGenerator for JavaExternalResolver {
    fn can_generate(&self, asset: &Path) -> bool {
        self.can_index(asset)
    }

    fn generate(
        &self,
        fqn: &str,
        entry: &AssetEntry,
    ) -> std::result::Result<IndexNode, Box<dyn std::error::Error + Send + Sync>> {
        self.generate_stub(fqn, &entry.path)
    }
}

impl AssetSourceLocator for JavaExternalResolver {
    fn locate_source(&self, entry: &AssetEntry) -> Option<PathBuf> {
        if !matches!(
            entry.source,
            AssetSource::Gradle { .. } | AssetSource::Maven { .. } | AssetSource::Local { .. }
        ) {
            return None;
        }
        let file_name = entry.path.file_name()?.to_string_lossy();
        if !file_name.ends_with(".jar") || file_name.ends_with("-sources.jar") {
            return None;
        }
        let mut source_name = file_name.to_string();
        source_name.truncate(source_name.len() - 4);
        source_name.push_str("-sources.jar");
        let source_path = entry.path.with_file_name(source_name);
        if source_path.exists() {
            Some(source_path)
        } else {
            None
        }
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
