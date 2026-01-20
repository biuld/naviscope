use crate::error::{NaviscopeError, Result};
use crate::model::lang::gradle::GradleDependency;
use tree_sitter::{Parser, QueryCursor, StreamingIterator};

unsafe extern "C" {
    fn tree_sitter_groovy() -> tree_sitter::Language;
}

use crate::parser::queries::gradle_definitions::GradleIndices;

pub fn parse_dependencies(source_code: &str) -> Result<Vec<GradleDependency>> {
    let mut parser = Parser::new();
    let language = unsafe { tree_sitter_groovy() };
    parser
        .set_language(&language)
        .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;

    let tree = parser
        .parse(source_code, None)
        .ok_or_else(|| NaviscopeError::Parsing("Failed to parse gradle file".to_string()))?;

    // 1. Load the unified query
    let query = crate::parser::utils::load_query(
        &language,
        include_str!("queries/gradle_definitions.scm"),
    )?;

    let indices = GradleIndices::new(&query)?;

    let mut query_cursor = QueryCursor::new();
    let mut item_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), source_code.as_bytes());

    let mut dependencies = Vec::new();

    while let Some(mat) = matches.next() {
        // Look for the dependencies block match
        let block_node = if let Some(cap) = mat.captures.iter().find(|c| c.index == indices.block) {
            cap.node
        } else {
            continue;
        };

        // 2. Query for items within the blocks
        let mut item_matches = item_cursor.matches(&query, block_node, source_code.as_bytes());

        while let Some(i_mat) = item_matches.next() {
            let string_node = if let Some(cap) = i_mat
                .captures
                .iter()
                .find(|c| c.index == indices.dep_string)
            {
                cap.node
            } else {
                continue;
            };

            // Parse content
            let range = string_node.byte_range();
            if range.end - range.start < 2 {
                continue;
            }
            let dependency_str = &source_code[range.start + 1..range.end - 1];

            let parts: Vec<&str> = dependency_str.split(':').collect();
            if parts.len() == 3 {
                dependencies.push(GradleDependency {
                    group: parts[0].to_string(),
                    name: parts[1].to_string(),
                    version: parts[2].to_string(),
                    id: String::new(),
                });
            }
        }
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_dependencies() {
        let gradle_file = r#"
            plugins {
                id 'java'
            }

            dependencies {
                implementation 'com.google.guava:guava:31.1-jre'
                testImplementation "org.junit.jupiter:junit-jupiter-api:5.8.2"
                api("org.apache.commons:commons-lang3:3.12.0")
            }

            // Should not be parsed
            otherBlock {
                implementation 'org.rogue:rogue-dependency:1.0'
            }
        "#;

        let dependencies = parse_dependencies(gradle_file).unwrap();
        assert_eq!(dependencies.len(), 3);

        assert_eq!(dependencies[0].group, "com.google.guava");
        assert_eq!(dependencies[0].name, "guava");
        assert_eq!(dependencies[0].version, "31.1-jre");

        assert_eq!(dependencies[1].group, "org.junit.jupiter");
        assert_eq!(dependencies[1].name, "junit-jupiter-api");
        assert_eq!(dependencies[1].version, "5.8.2");

        assert_eq!(dependencies[2].group, "org.apache.commons");
        assert_eq!(dependencies[2].name, "commons-lang3");
        assert_eq!(dependencies[2].version, "3.12.0");
    }
}
