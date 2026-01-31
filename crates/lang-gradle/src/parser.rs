use crate::model::{GradleDependency, GradleSettings};
use naviscope_core::error::{NaviscopeError, Result};
use once_cell::sync::Lazy;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

unsafe extern "C" {
    fn tree_sitter_groovy() -> tree_sitter::Language;
}

use crate::queries::gradle_definitions::GradleIndices;

/// Cached Gradle query to avoid re-parsing the SCM file on every parse call.
static GRADLE_QUERY: Lazy<Query> = Lazy::new(|| {
    let language = unsafe { tree_sitter_groovy() };
    naviscope_core::parser::utils::load_query(
        &language,
        include_str!("queries/gradle_definitions.scm"),
    )
    .expect("Failed to load Gradle query - this is a fatal error")
});

/// Gets the cached Gradle query.
fn get_gradle_query() -> &'static Query {
    &GRADLE_QUERY
}

pub fn parse_dependencies(source_code: &str) -> Result<Vec<GradleDependency>> {
    let mut parser = Parser::new();
    let language = unsafe { tree_sitter_groovy() };
    parser
        .set_language(&language)
        .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;

    let tree = parser
        .parse(source_code, None)
        .ok_or_else(|| NaviscopeError::Parsing("Failed to parse gradle file".to_string()))?;

    let query = get_gradle_query();

    let indices = GradleIndices::new(query)?;

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), source_code.as_bytes());

    let mut dependencies = Vec::new();

    while let Some(mat) = matches.next() {
        // 1. External dependencies
        if let Some(_cap) = mat.captures.iter().find(|c| c.index == indices.item) {
            if let Some(str_cap) = mat.captures.iter().find(|c| c.index == indices.dep_string) {
                let range = str_cap.node.byte_range();
                if range.end - range.start >= 2 {
                    let dependency_str = &source_code[range.start + 1..range.end - 1];
                    let parts: Vec<&str> = dependency_str.split(':').collect();
                    if parts.len() == 3 {
                        dependencies.push(GradleDependency {
                            group: Some(parts[0].to_string()),
                            name: parts[1].to_string(),
                            version: Some(parts[2].to_string()),
                            is_project: false,
                            id: String::new(),
                        });
                    }
                }
            }
        }

        // 2. Project dependencies
        if let Some(_cap) = mat
            .captures
            .iter()
            .find(|c| c.index == indices.project_item)
        {
            if let Some(path_cap) = mat
                .captures
                .iter()
                .find(|c| c.index == indices.project_path)
            {
                let range = path_cap.node.byte_range();
                if range.end - range.start >= 2 {
                    let project_path = &source_code[range.start + 1..range.end - 1];
                    dependencies.push(GradleDependency {
                        group: None,
                        name: project_path.to_string(),
                        version: None,
                        is_project: true,
                        id: String::new(),
                    });
                }
            }
        }
    }

    Ok(dependencies)
}

pub fn parse_settings(source_code: &str) -> Result<GradleSettings> {
    let mut parser = Parser::new();
    let language = unsafe { tree_sitter_groovy() };
    parser
        .set_language(&language)
        .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;

    let tree = parser.parse(source_code, None).ok_or_else(|| {
        NaviscopeError::Parsing("Failed to parse gradle settings file".to_string())
    })?;

    let query = get_gradle_query();

    let indices = GradleIndices::new(query)?;

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), source_code.as_bytes());

    let mut root_project_name = None;
    let mut included_projects = Vec::new();

    while let Some(mat) = matches.next() {
        // Root project name
        let mut found_root = false;
        if let Some(_) = mat
            .captures
            .iter()
            .find(|c| c.index == indices.root_assignment)
        {
            found_root = true;
        }

        if found_root {
            if let Some(name_cap) = mat.captures.iter().find(|c| c.index == indices.root_name) {
                let range = name_cap.node.byte_range();
                if range.end - range.start >= 2 {
                    root_project_name =
                        Some(source_code[range.start + 1..range.end - 1].to_string());
                }
            }
        }

        // Included projects
        if let Some(_) = mat
            .captures
            .iter()
            .find(|c| c.index == indices.include_call)
        {
            if let Some(path_cap) = mat
                .captures
                .iter()
                .find(|c| c.index == indices.included_path)
            {
                let range = path_cap.node.byte_range();
                if range.end - range.start >= 2 {
                    included_projects.push(source_code[range.start + 1..range.end - 1].to_string());
                }
            }
        }
    }

    Ok(GradleSettings {
        root_project_name,
        included_projects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_dependencies() {
        let gradle_file = r#"
            dependencies {
                implementation 'com.google.guava:guava:31.1-jre'
                testImplementation "org.junit.jupiter:junit-jupiter-api:5.8.2"
                implementation project(':core:spring-boot')
            }
        "#;

        let dependencies = parse_dependencies(gradle_file).unwrap();
        assert_eq!(dependencies.len(), 3);

        assert_eq!(dependencies[0].group, Some("com.google.guava".to_string()));
        assert_eq!(dependencies[0].name, "guava");
        assert_eq!(dependencies[0].is_project, false);

        assert_eq!(dependencies[2].name, ":core:spring-boot");
        assert_eq!(dependencies[2].is_project, true);
    }

    #[test]
    fn test_parse_settings() {
        let settings_file = r#"
            rootProject.name = 'spring-boot-build'
            include 'core:spring-boot'
            include 'module:spring-boot-actuator'
        "#;

        let settings = parse_settings(settings_file).unwrap();
        assert_eq!(
            settings.root_project_name,
            Some("spring-boot-build".to_string())
        );
        assert_eq!(settings.included_projects.len(), 2);
        assert_eq!(settings.included_projects[0], "core:spring-boot");
    }

    #[test]
    fn test_parse_settings_multi_include() {
        let settings_file = r#"
            include 'a', 'b', "c"
            include 'd'
        "#;

        let settings = parse_settings(settings_file).unwrap();
        assert_eq!(settings.included_projects.len(), 4);
        assert!(settings.included_projects.contains(&"a".to_string()));
        assert!(settings.included_projects.contains(&"b".to_string()));
        assert!(settings.included_projects.contains(&"c".to_string()));
        assert!(settings.included_projects.contains(&"d".to_string()));
    }

    #[test]
    fn test_parse_complex_spring_boot_settings() {
        let settings_file = r#"
            pluginManagement {
                repositories {
                    gradlePluginPortal()
                }
            }
            rootProject.name = "spring-boot-build"
            include "spring-boot-project:spring-boot"
            include "spring-boot-project:spring-boot-actuator"
            include ":smoke-test:spring-boot-smoke-test-xml"
        "#;

        let settings = parse_settings(settings_file).unwrap();

        assert_eq!(
            settings.root_project_name,
            Some("spring-boot-build".to_string()),
            "Failed to parse double-quoted rootProject.name"
        );

        assert!(
            settings
                .included_projects
                .contains(&"spring-boot-project:spring-boot".to_string()),
            "Failed to parse double-quoted include"
        );
        assert!(
            settings
                .included_projects
                .contains(&":smoke-test:spring-boot-smoke-test-xml".to_string())
        );
    }
}
