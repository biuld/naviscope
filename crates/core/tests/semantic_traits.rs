use naviscope_api::models::{
    DisplayGraphNode, DisplaySymbolLocation, Language, NodeKind, Range, ReferenceQuery,
    SymbolQuery, SymbolResolution,
};
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, SymbolInfoProvider, SymbolNavigator,
};
use naviscope_core::engine::{EngineHandle, NaviscopeEngine};
use naviscope_core::model::ResolvedUnit;
use naviscope_core::parser::{GlobalParseResult, LspParser};
use naviscope_core::plugin::{LanguageFeatureProvider, LanguagePlugin, MetadataPlugin};
use naviscope_core::project::scanner::ParsedFile;
use naviscope_core::query::CodeGraphLike;
use naviscope_core::resolver::{LangResolver, ProjectContext, SemanticResolver};
use petgraph::stable_graph::NodeIndex;
use std::path::Path;
use std::sync::Arc;
use tree_sitter::Tree;

struct MockPlugin {
    resolver: Arc<MockResolver>,
    lsp_parser: Arc<MockLspParser>,
    lang_resolver: Arc<MockLangResolver>,
}

impl MetadataPlugin for MockPlugin {}

impl LanguagePlugin for MockPlugin {
    fn name(&self) -> Language {
        Language::new("mock")
    }
    fn supported_extensions(&self) -> &[&str] {
        &["mock"]
    }
    fn parse_file(
        &self,
        _source: &str,
        _path: &Path,
    ) -> naviscope_core::error::Result<GlobalParseResult> {
        Ok(GlobalParseResult {
            package_name: None,
            imports: vec![],
            nodes: vec![],
            relations: vec![],
            source: Some(_source.to_string()),
            tree: None,
            identifiers: vec!["Callee".to_string()],
        })
    }
    fn resolver(&self) -> Arc<dyn SemanticResolver> {
        self.resolver.clone()
    }
    fn lang_resolver(&self) -> Arc<dyn LangResolver> {
        self.lang_resolver.clone()
    }
    fn lsp_parser(&self) -> Arc<dyn LspParser> {
        self.lsp_parser.clone()
    }
    fn feature_provider(&self) -> Arc<dyn LanguageFeatureProvider> {
        Arc::new(MockFeatureProvider)
    }
}

struct MockLangResolver {
    nodes: std::sync::Mutex<Vec<DisplayGraphNode>>,
}

impl LangResolver for MockLangResolver {
    fn resolve(
        &self,
        file: &ParsedFile,
        _context: &ProjectContext,
    ) -> naviscope_core::error::Result<ResolvedUnit> {
        let mut unit = ResolvedUnit::new();

        let identifiers = match &file.content {
            naviscope_core::project::scanner::ParsedContent::Language(res) => {
                res.identifiers.clone()
            }
            naviscope_core::project::scanner::ParsedContent::Unparsed(_src) => {
                vec!["Callee".to_string()]
            }
            _ => vec![],
        };

        if !identifiers.is_empty() {
            unit.identifiers = identifiers.clone();
            unit.ops
                .push(naviscope_core::model::GraphOp::UpdateIdentifiers {
                    path: file.file.path.clone().into(),
                    identifiers: unit.identifiers.clone(),
                });
        }

        let nodes = self.nodes.lock().unwrap();
        for node in nodes.iter() {
            unit.add_node(node.clone());
        }
        Ok(unit)
    }
}

struct MockFeatureProvider;
impl LanguageFeatureProvider for MockFeatureProvider {
    fn detail_view(&self, node: &DisplayGraphNode) -> Option<String> {
        Some(format!("Mock detail for {}", node.id))
    }
    fn signature(&self, node: &DisplayGraphNode) -> Option<String> {
        Some(format!("Mock signature for {}", node.id))
    }
    fn modifiers(&self, _node: &DisplayGraphNode) -> Vec<String> {
        vec!["mock".to_string()]
    }
}

struct MockResolver {
    res_at: std::sync::Mutex<Option<SymbolResolution>>,
}

impl SemanticResolver for MockResolver {
    fn resolve_at(
        &self,
        _tree: &Tree,
        _source: &str,
        _line: usize,
        _byte_col: usize,
        _index: &dyn CodeGraphLike,
    ) -> Option<SymbolResolution> {
        self.res_at.lock().unwrap().clone()
    }

    fn find_matches(&self, index: &dyn CodeGraphLike, res: &SymbolResolution) -> Vec<NodeIndex> {
        if let SymbolResolution::Global(id) = res {
            if let Some(idx) = index.find_node(id.as_str()) {
                return vec![idx];
            }
        }
        vec![]
    }

    fn resolve_type_of(
        &self,
        _index: &dyn CodeGraphLike,
        _res: &SymbolResolution,
    ) -> Vec<SymbolResolution> {
        vec![SymbolResolution::Global("test::Type".to_string())]
    }

    fn find_implementations(
        &self,
        index: &dyn CodeGraphLike,
        _res: &SymbolResolution,
    ) -> Vec<NodeIndex> {
        if let Some(idx) = index.find_node("test::Impl") {
            return vec![idx];
        }
        vec![]
    }
}

struct MockLspParser;
impl LspParser for MockLspParser {
    fn parse(&self, source: &str, _old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .ok()?;
        parser.parse(source, None)
    }
    fn extract_symbols(
        &self,
        _tree: &Tree,
        _source: &str,
    ) -> Vec<naviscope_api::models::DocumentSymbol> {
        vec![]
    }
    fn symbol_kind(&self, _kind: &naviscope_core::model::NodeKind) -> lsp_types::SymbolKind {
        lsp_types::SymbolKind::CLASS
    }
    fn find_occurrences(
        &self,
        _source: &str,
        _tree: &Tree,
        target: &SymbolResolution,
    ) -> Vec<Range> {
        if let SymbolResolution::Global(id) = target {
            if id == "test::Callee" {
                return vec![Range {
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: 5,
                }];
            }
        }
        vec![]
    }
}

fn setup_engine(temp_dir: &Path) -> (NaviscopeEngine, Arc<MockPlugin>) {
    let mut engine = NaviscopeEngine::new(temp_dir.to_path_buf());
    let mock_resolver = Arc::new(MockResolver {
        res_at: std::sync::Mutex::new(None),
    });
    let mock_parser = Arc::new(MockLspParser);
    let mock_lang_resolver = Arc::new(MockLangResolver {
        nodes: std::sync::Mutex::new(vec![]),
    });
    let plugin = Arc::new(MockPlugin {
        resolver: mock_resolver,
        lsp_parser: mock_parser,
        lang_resolver: mock_lang_resolver,
    });
    engine.register_language(plugin.clone());
    (engine, plugin)
}

#[tokio::test]
async fn test_symbol_navigator_queries() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_navigator_real");
    std::fs::create_dir_all(&temp_dir).ok();

    let (engine, plugin) = setup_engine(&temp_dir);

    // Add a node to the mock plugin's resolver
    {
        let mut nodes = plugin.lang_resolver.nodes.lock().unwrap();
        nodes.push(DisplayGraphNode {
            id: "test::Symbol".to_string(),
            name: "Symbol".to_string(),
            kind: NodeKind::Class,
            lang: "mock".to_string(),
            location: Some(DisplaySymbolLocation {
                path: temp_dir.join("test.mock").to_string_lossy().to_string(),
                range: Range {
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 10,
                },
                selection_range: None,
            }),
            metadata: serde_json::Value::Null,
        });
    }

    let test_file = temp_dir.join("test.mock");
    std::fs::write(&test_file, "mock content").unwrap();

    // Trigger update to populate graph
    engine.update_files(vec![test_file.clone()]).await.unwrap();

    let handle = EngineHandle::from_engine(Arc::new(engine));

    // Test find_definitions
    let query = SymbolQuery {
        language: Language::new("mock"),
        resolution: SymbolResolution::Global("test::Symbol".to_string()),
    };

    let defs = handle.find_definitions(&query).await.unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].path.as_os_str(), test_file.as_os_str());
}

#[tokio::test]
async fn test_reference_analyzer() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_references");
    std::fs::create_dir_all(&temp_dir).ok();

    let (engine, _) = setup_engine(&temp_dir);
    let handle = EngineHandle::from_engine(Arc::new(engine));

    let query = ReferenceQuery {
        language: Language::new("mock"),
        resolution: SymbolResolution::Global("test::Symbol".to_string()),
        include_declaration: true,
    };

    let refs = handle.find_references(&query).await.unwrap();
    assert!(refs.is_empty());
}

#[tokio::test]
async fn test_symbol_info_provider() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_info");
    std::fs::create_dir_all(&temp_dir).ok();

    let (engine, _) = setup_engine(&temp_dir);
    let handle = EngineHandle::from_engine(Arc::new(engine));

    let test_file = temp_dir.join("test.mock");
    std::fs::write(&test_file, "mock content").unwrap();
    let uri = format!("file://{}", test_file.display());

    let lang = handle.get_language_for_document(&uri).await.unwrap();
    assert_eq!(lang, Some(Language::new("mock")));

    let symbols = handle.get_document_symbols(&uri).await.unwrap();
    assert!(symbols.is_empty());
}

#[tokio::test]
async fn test_call_hierarchy_analyzer() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_hierarchy");
    std::fs::create_dir_all(&temp_dir).ok();

    let (engine, plugin) = setup_engine(&temp_dir);

    let test_file = temp_dir.join("test.mock");
    let test_file_path = test_file.to_string_lossy().to_string();

    // Add caller and callee nodes
    {
        let mut nodes = plugin.lang_resolver.nodes.lock().unwrap();
        // Callee
        nodes.push(DisplayGraphNode {
            id: "test::Callee".to_string(),
            name: "Callee".to_string(),
            kind: NodeKind::Method,
            lang: "mock".to_string(),
            location: Some(DisplaySymbolLocation {
                path: test_file_path.clone(),
                range: Range {
                    start_line: 5,
                    start_col: 0,
                    end_line: 5,
                    end_col: 10,
                },
                selection_range: None,
            }),
            metadata: serde_json::Value::Null,
        });
        // Caller
        nodes.push(DisplayGraphNode {
            id: "test::Caller".to_string(),
            name: "Caller".to_string(),
            kind: NodeKind::Method,
            lang: "mock".to_string(),
            location: Some(DisplaySymbolLocation {
                path: test_file_path.clone(),
                range: Range {
                    start_line: 0,
                    start_col: 0,
                    end_line: 2,
                    end_col: 10,
                },
                selection_range: None,
            }),
            metadata: serde_json::Value::Null,
        });
    }

    std::fs::write(&test_file, "caller calls callee").unwrap();
    engine.update_files(vec![test_file.clone()]).await.unwrap();

    // Set mock resolution for verification (needed by scan_file)
    *plugin.resolver.res_at.lock().unwrap() =
        Some(SymbolResolution::Global("test::Callee".to_string()));

    let handle = EngineHandle::from_engine(Arc::new(engine));

    // 1. Test Incoming Calls (Who calls Callee?)
    let incoming = handle.find_incoming_calls("test::Callee").await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from.id, "test::Caller");
    assert_eq!(incoming[0].from_ranges.len(), 1);
    assert_eq!(incoming[0].from_ranges[0].start_line, 1);

    // 2. Test Outgoing Calls (Who does Caller call?)
    let outgoing = handle.find_outgoing_calls("test::Caller").await.unwrap();
    assert!(!outgoing.is_empty());
    assert_eq!(outgoing[0].to.id, "test::Callee");
}

#[tokio::test]
async fn test_get_symbol_info() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_symbol_info_final");
    std::fs::create_dir_all(&temp_dir).ok();

    let (engine, plugin) = setup_engine(&temp_dir);

    // Add a node to the mock plugin's resolver
    {
        let mut nodes = plugin.lang_resolver.nodes.lock().unwrap();
        nodes.push(DisplayGraphNode {
            id: "test::Symbol".to_string(),
            name: "Symbol".to_string(),
            kind: NodeKind::Class,
            lang: "mock".to_string(),
            location: Some(DisplaySymbolLocation {
                path: temp_dir.join("test.mock").to_string_lossy().to_string(),
                range: Range {
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 10,
                },
                selection_range: None,
            }),
            metadata: serde_json::Value::Null,
        });
    }

    let test_file = temp_dir.join("test.mock");
    std::fs::write(&test_file, "mock content").unwrap();

    // Trigger update to populate graph
    engine.update_files(vec![test_file.clone()]).await.unwrap();

    let handle = EngineHandle::from_engine(Arc::new(engine));

    // Test get_symbol_info
    let info = handle.get_symbol_info("test::Symbol").await.unwrap();
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.name, "Symbol");
    assert_eq!(
        info.detail,
        Some("Mock detail for test::Symbol".to_string())
    );
    assert_eq!(
        info.signature,
        Some("Mock signature for test::Symbol".to_string())
    );
    assert_eq!(info.language.as_str(), "mock");
}
