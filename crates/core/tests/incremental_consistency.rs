use naviscope_api::models::{Language, PositionContext, ReferenceQuery, SymbolQuery, SymbolResolution};
use naviscope_api::semantic::{ReferenceAnalyzer, SymbolInfoProvider, SymbolNavigator};
use naviscope_core::facade::EngineHandle;
use naviscope_core::runtime::NaviscopeEngine as CoreEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Once;

fn ensure_test_index_dir() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join("naviscope_test_index_dir_incremental_consistency");
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("NAVISCOPE_INDEX_DIR", dir);
        }
    });
}

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in content.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16();
        }
    }
    (line, col)
}

fn normalize_path(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    resolved_fqn: String,
    defs: Vec<(String, usize, usize, usize, usize)>,
    refs: Vec<(String, usize, usize, usize, usize)>,
    symbol_id: String,
    symbol_source: naviscope_api::models::NodeSource,
}

async fn observe(
    engine: Arc<CoreEngine>,
    use_path: &Path,
    use_content: &str,
) -> Observation {
    let handle = EngineHandle::from_engine(engine);
    let call_offset = use_content
        .find("target();")
        .expect("use file should contain target() call");
    let (line, col) = offset_to_point(use_content, call_offset);

    let ctx = PositionContext {
        uri: format!("file://{}", use_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(use_content.to_string()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("callsite should resolve");

    let resolved_fqn = match &resolution {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => fqn.clone(),
        SymbolResolution::Local(_, _) => panic!("method call should not resolve as local"),
    };

    let query = SymbolQuery {
        language: Language::JAVA,
        resolution: resolution.clone(),
    };

    let mut defs: Vec<_> = handle
        .find_definitions(&query)
        .await
        .unwrap()
        .into_iter()
        .map(|loc| {
            (
                normalize_path(&loc.path),
                loc.range.start_line,
                loc.range.start_col,
                loc.range.end_line,
                loc.range.end_col,
            )
        })
        .collect();
    defs.sort();

    let mut refs: Vec<_> = handle
        .find_references(&ReferenceQuery {
            language: Language::JAVA,
            resolution,
            include_declaration: false,
        })
        .await
        .unwrap()
        .into_iter()
        .map(|loc| {
            (
                normalize_path(&loc.path),
                loc.range.start_line,
                loc.range.start_col,
                loc.range.end_line,
                loc.range.end_col,
            )
        })
        .collect();
    refs.sort();

    let info = handle
        .get_symbol_info(&resolved_fqn)
        .await
        .unwrap()
        .expect("resolved symbol should have display info");

    Observation {
        resolved_fqn,
        defs,
        refs,
        symbol_id: info.id,
        symbol_source: info.source,
    }
}

#[tokio::test]
async fn test_incremental_vs_full_consistency_for_java_navigation() {
    ensure_test_index_dir();
    let full_dir = std::env::temp_dir().join("naviscope_full_consistency_java");
    let inc_dir = std::env::temp_dir().join("naviscope_inc_consistency_java");

    for dir in [&full_dir, &inc_dir] {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(dir);
        }
        std::fs::create_dir_all(dir.join("com/example")).unwrap();
    }

    let a_rel = PathBuf::from("com/example/A.java");
    let use_rel = PathBuf::from("com/example/Use.java");
    let a_src = "package com.example; public class A { void target() {} }";
    let use_src = r#"
package com.example;
public class Use {
    void run() {
        new A().target();
    }
}
"#;

    std::fs::write(full_dir.join(&a_rel), a_src).unwrap();
    std::fs::write(full_dir.join(&use_rel), use_src).unwrap();
    std::fs::write(inc_dir.join(&a_rel), a_src).unwrap();
    std::fs::write(inc_dir.join(&use_rel), use_src).unwrap();

    let java_caps_full = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine_full = Arc::new(
        CoreEngine::builder(full_dir.clone())
            .with_language_caps(java_caps_full)
            .build(),
    );
    engine_full
        .update_files(vec![full_dir.join(&a_rel), full_dir.join(&use_rel)])
        .await
        .unwrap();
    let full_obs = observe(Arc::clone(&engine_full), &full_dir.join(&use_rel), use_src).await;

    let java_caps_inc = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine_inc = Arc::new(
        CoreEngine::builder(inc_dir.clone())
            .with_language_caps(java_caps_inc)
            .build(),
    );
    engine_inc
        .update_files(vec![inc_dir.join(&a_rel)])
        .await
        .unwrap();
    engine_inc
        .update_files(vec![inc_dir.join(&use_rel)])
        .await
        .unwrap();
    let inc_obs = observe(Arc::clone(&engine_inc), &inc_dir.join(&use_rel), use_src).await;

    assert_eq!(full_obs, inc_obs, "incremental and full observations must match");

    let _ = std::fs::remove_dir_all(&full_dir);
    let _ = std::fs::remove_dir_all(&inc_dir);
}
