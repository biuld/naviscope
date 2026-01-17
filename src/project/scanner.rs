use super::source::{BuildTool, Language, SourceFile};
use crate::model::lang::gradle::GradleParseResult;
use crate::parser::gradle;
use crate::parser::java::{JavaParseResult, JavaParser};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use xxhash_rust::xxh3::Xxh3;

pub enum ParsedContent {
    Java(JavaParseResult),
    Gradle(GradleParseResult),
}

pub struct ParsedFile {
    pub file: SourceFile,
    pub content: ParsedContent,
}

impl ParsedFile {
    pub fn is_build(&self) -> bool {
        matches!(self.content, ParsedContent::Gradle(..))
    }

    pub fn build_tool(&self) -> Option<BuildTool> {
        match self.content {
            ParsedContent::Gradle(..) => Some(BuildTool::Gradle),
            _ => None,
        }
    }

    pub fn language(&self) -> Option<Language> {
        match self.content {
            ParsedContent::Java(..) => Some(Language::Java),
            ParsedContent::Gradle(..) => Some(Language::BuildFile),
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

                if file_name == "build.gradle" || file_name == "build.gradle.kts" {
                    let deps = gradle::parse_dependencies(&content_str).ok()?;
                    Some(ParsedFile {
                        file: source_file,
                        content: ParsedContent::Gradle(GradleParseResult { dependencies: deps }),
                    })
                } else if extension == "java" {
                    let parser = JavaParser::new().ok()?;
                    let res = parser
                        .parse_file(&content_str, Some(&source_file.path))
                        .ok()?;
                    Some(ParsedFile {
                        file: source_file,
                        content: ParsedContent::Java(res),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn collect_paths(root: &Path) -> Vec<std::path::PathBuf> {
        let walker = WalkBuilder::new(root)
            .git_ignore(true)
            .hidden(false)
            .build();

        walker
            .filter_map(|result| match result {
                Ok(entry) if entry.file_type().map_or(false, |ft| ft.is_file()) => {
                    Some(entry.into_path())
                }
                _ => None,
            })
            .collect()
    }

    fn process_file_with_mtime(path: &Path, modified: u64) -> Option<(SourceFile, Vec<u8>)> {
        let content = fs::read(path).ok()?;
        let mut hasher = Xxh3::new();
        hasher.write(&content);
        let hash = hasher.finish();

        Some((SourceFile::new(path.to_path_buf(), hash, modified), content))
    }
}
