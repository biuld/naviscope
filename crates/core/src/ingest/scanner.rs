use super::is_relevant_path;
use crate::ingest::parser::GlobalParseResult;
use crate::model::source::{BuildTool, Language, SourceFile};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use xxhash_rust::xxh3::Xxh3;

#[derive(Clone)]
pub enum ParsedContent {
    Language(GlobalParseResult),
    MetaData(serde_json::Value),
    Unparsed(String),
}

#[derive(Clone)]
pub struct ParsedFile {
    pub file: SourceFile,
    pub content: ParsedContent,
}

impl ParsedFile {
    pub fn is_build(&self) -> bool {
        match self.content {
            ParsedContent::Unparsed(..) => {
                let name = self
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                name == "build.gradle"
                    || name == "build.gradle.kts"
                    || name == "settings.gradle"
                    || name == "settings.gradle.kts"
            }
            _ => false,
        }
    }

    pub fn build_tool(&self) -> Option<BuildTool> {
        match self.content {
            ParsedContent::Unparsed(..) => {
                if self.is_build() {
                    Some(BuildTool::GRADLE)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn language(&self) -> Option<Language> {
        match self.content {
            ParsedContent::Language(ref res) => {
                // Try to infer from package
                if let Some(ref pkg) = res.package_name {
                    if pkg.starts_with("java.") || pkg.starts_with("javax.") {
                        return Some(Language::JAVA);
                    }
                }
                Some(Language::UNKNOWN)
            }
            ParsedContent::MetaData(..) => None,
            ParsedContent::Unparsed(..) => {
                if self.is_build() {
                    self.build_tool()
                        .map(|t| Language::new(t.as_str().to_string()))
                } else {
                    self.path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .and_then(Language::from_extension)
                }
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.file.path
    }
}

pub struct Scanner;

impl Scanner {
    pub fn scan_and_parse(
        root: &Path,
        existing_files: &HashMap<PathBuf, SourceFile>,
    ) -> Vec<ParsedFile> {
        let paths = Self::collect_paths(root);
        Self::scan_files(paths, existing_files)
    }

    pub fn scan_files(
        paths: Vec<PathBuf>,
        existing_files: &HashMap<PathBuf, SourceFile>,
    ) -> Vec<ParsedFile> {
        paths
            .par_iter()
            .filter_map(|path| {
                // 1. Check metadata (mtime) first
                let metadata = fs::metadata(path).ok()?;
                let modified = metadata
                    .modified()
                    .unwrap_or(SystemTime::UNIX_EPOCH)
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_secs();

                if let Some(existing) = existing_files.get(path) {
                    if existing.last_modified == modified {
                        return None;
                    }
                }

                // 2. Read and hash content
                let (source_file, content) = Self::process_file_with_mtime(path, modified)?;

                // 3. Double check hash (mtime might change but content remains same)
                if let Some(existing) = existing_files.get(path) {
                    if existing.content_hash == source_file.content_hash {
                        return None;
                    }
                }

                let content_str = String::from_utf8(content).ok()?;

                // Determine build tool or language from file extension/name
                let file_name = path.file_name()?.to_str()?;

                if file_name == "build.gradle"
                    || file_name == "build.gradle.kts"
                    || file_name == "settings.gradle"
                    || file_name == "settings.gradle.kts"
                    || path.extension().is_some()
                {
                    Some(ParsedFile {
                        file: source_file,
                        content: ParsedContent::Unparsed(content_str),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn collect_paths(root: &Path) -> Vec<PathBuf> {
        WalkBuilder::new(root)
            .build()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() && is_relevant_path(path) {
                    return Some(path.to_path_buf());
                }
                None
            })
            .collect()
    }

    fn process_file_with_mtime(path: &Path, mtime: u64) -> Option<(SourceFile, Vec<u8>)> {
        let content = fs::read(path).ok()?;
        let mut hasher = Xxh3::new();
        hasher.write(&content);
        let hash = hasher.finish();

        Some((
            SourceFile {
                path: path.to_path_buf(),
                content_hash: hash,
                last_modified: mtime,
            },
            content,
        ))
    }
}
