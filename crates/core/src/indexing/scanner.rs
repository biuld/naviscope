use super::is_relevant_path;

use crate::model::source::SourceFile;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use xxhash_rust::xxh3::Xxh3;

pub use naviscope_plugin::{ParsedContent, ParsedFile};

pub struct Scanner;

impl Scanner {
    pub fn scan_files_iter<'a>(
        paths: Vec<PathBuf>,
        existing_files: &'a HashMap<PathBuf, SourceFile>,
    ) -> impl Iterator<Item = ParsedFile> + 'a {
        paths
            .into_iter()
            .filter_map(|path| Self::parse_path(&path, existing_files))
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

    fn process_file_with_mtime(path: &Path, mtime: u64) -> Option<SourceFile> {
        let content = fs::read(path).ok()?;
        let mut hasher = Xxh3::new();
        hasher.write(&content);
        let hash = hasher.finish();

        Some(SourceFile {
            path: path.to_path_buf(),
            content_hash: hash,
            last_modified: mtime,
        })
    }

    fn parse_path(path: &Path, existing_files: &HashMap<PathBuf, SourceFile>) -> Option<ParsedFile> {
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
        let source_file = Self::process_file_with_mtime(path, modified)?;

        // 3. Double check hash (mtime might change but content remains same)
        if let Some(existing) = existing_files.get(path) {
            if existing.content_hash == source_file.content_hash {
                return None;
            }
        }

        if path.extension().is_some() {
            Some(ParsedFile {
                file: source_file,
                content: ParsedContent::Lazy,
            })
        } else {
            None
        }
    }
}
