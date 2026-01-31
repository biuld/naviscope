use super::is_relevant_path;
use super::source::{BuildTool, Language, SourceFile};
use crate::parser::GlobalParseResult;
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
                    Some(BuildTool::Gradle)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn language(&self) -> Option<Language> {
        match self.content {
            ParsedContent::Language(..) => Some(Language::Java), // Still assuming Java for now if it's Language
            ParsedContent::MetaData(..) => None,
            ParsedContent::Unparsed(..) => {
                if self.is_build() {
                    Some(Language::BuildFile)
                } else if self.path().extension().map_or(false, |e| e == "java") {
                    Some(Language::Java)
                } else {
                    None
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
        Self::collect_paths(root)
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
                let extension = path.extension()?.to_str()?;

                if file_name == "build.gradle"
                    || file_name == "build.gradle.kts"
                    || file_name == "settings.gradle"
                    || file_name == "settings.gradle.kts"
                    || extension == "java"
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
