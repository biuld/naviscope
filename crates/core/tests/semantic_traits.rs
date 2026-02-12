use naviscope_api::models::{
    DisplayGraphNode, DisplaySymbolLocation, Language, NodeKind, NodeSource, Range, SymbolQuery,
    SymbolResolution,
};
use naviscope_api::semantic::{SymbolInfoProvider, SymbolNavigator};
use naviscope_core::facade::EngineHandle;
use naviscope_core::runtime::orchestrator::NaviscopeEngine as CoreEngine;
use naviscope_plugin::{
    AssetCap, CodecContext, FileMatcherCap, GlobalParseResult, LanguageCaps, LanguageParseCap,
    LspSyntaxService, MetadataCodecCap, NamingConvention, NodeMetadataCodec, NodePresenter,
    ParsedContent, ParsedFile, PresentationCap, ProjectContext, ReferenceCheckService,
    ResolvedUnit, SemanticCap, SourceIndexCap, StandardNamingConvention, SymbolQueryService,
    SymbolResolveService,
};
use std::path::Path;
use std::sync::Arc;
use std::sync::Once;
use tree_sitter::Tree;

#[derive(Clone)]
struct MockCap;

impl FileMatcherCap for MockCap {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("mock")
    }
}

impl LanguageParseCap for MockCap {
    fn parse_language_file(
        &self,
        source: &str,
        _path: &Path,
    ) -> Result<GlobalParseResult, naviscope_plugin::BoxError> {
        Ok(GlobalParseResult {
            package_name: None,
            imports: vec![],
            output: naviscope_plugin::ParseOutput {
                nodes: vec![],
                relations: vec![],
                identifiers: vec!["Symbol".to_string()],
            },
            source: Some(source.to_string()),
            tree: None,
        })
    }
}

impl SourceIndexCap for MockCap {
    fn compile_source(
        &self,
        file: &ParsedFile,
        _context: &ProjectContext,
    ) -> Result<ResolvedUnit, naviscope_plugin::BoxError> {
        let mut unit = ResolvedUnit::new();
        let identifiers = match &file.content {
            ParsedContent::Language(res) => res.output.identifiers.clone(),
            _ => vec!["Symbol".to_string()],
        };
        unit.identifiers = identifiers.clone();
        unit.ops.push(naviscope_plugin::GraphOp::UpdateIdentifiers {
            path: Arc::from(file.file.path.as_path()),
            identifiers,
        });
        unit.add_node(naviscope_plugin::IndexNode {
            id: naviscope_api::models::symbol::NodeId::Flat("test::Symbol".to_string()),
            name: "Symbol".to_string(),
            kind: NodeKind::Class,
            lang: "mock".to_string(),
            source: NodeSource::Project,
            status: naviscope_api::models::graph::ResolutionStatus::Resolved,
            location: Some(DisplaySymbolLocation {
                path: file.path().to_string_lossy().to_string(),
                range: Range {
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 10,
                },
                selection_range: None,
            }),
            metadata: Arc::new(naviscope_api::models::graph::EmptyMetadata),
        });
        Ok(unit)
    }
}

impl SymbolResolveService for MockCap {
    fn resolve_at(
        &self,
        _tree: &Tree,
        _source: &str,
        _line: usize,
        _byte_col: usize,
        _index: &dyn naviscope_plugin::CodeGraph,
    ) -> Option<SymbolResolution> {
        Some(SymbolResolution::Global("test::Symbol".to_string()))
    }
}

impl SymbolQueryService for MockCap {
    fn find_matches(
        &self,
        index: &dyn naviscope_plugin::CodeGraph,
        res: &SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::FqnId> {
        match res {
            SymbolResolution::Global(fqn) => index.resolve_fqn(fqn),
            SymbolResolution::Precise(fqn, _) => index.resolve_fqn(fqn),
            SymbolResolution::Local(_, _) => vec![],
        }
    }

    fn resolve_type_of(
        &self,
        _index: &dyn naviscope_plugin::CodeGraph,
        _res: &SymbolResolution,
    ) -> Vec<SymbolResolution> {
        vec![]
    }

    fn find_implementations(
        &self,
        _index: &dyn naviscope_plugin::CodeGraph,
        _res: &SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::FqnId> {
        vec![]
    }
}

impl LspSyntaxService for MockCap {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .ok()?;
        parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, _tree: &Tree, _source: &str) -> Vec<DisplayGraphNode> {
        vec![]
    }

    fn find_occurrences(
        &self,
        _source: &str,
        _tree: &Tree,
        _target: &SymbolResolution,
        _index: Option<&dyn naviscope_plugin::CodeGraph>,
    ) -> Vec<Range> {
        vec![]
    }
}

impl ReferenceCheckService for MockCap {
    fn is_reference_to(
        &self,
        _graph: &dyn naviscope_plugin::CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool {
        candidate == target
    }
}

impl AssetCap for MockCap {}

impl NodePresenter for MockCap {
    fn render_display_node(
        &self,
        node: &naviscope_api::models::graph::GraphNode,
        fqns: &dyn naviscope_api::models::symbol::FqnReader,
    ) -> DisplayGraphNode {
        DisplayGraphNode {
            id: StandardNamingConvention.render_fqn(node.id, fqns),
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: fqns.resolve_atom(node.lang).to_string(),
            source: node.source.clone(),
            status: node.status,
            location: node.location.as_ref().map(|l| l.to_display(fqns)),
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        }
    }
}

impl NodeMetadataCodec for MockCap {
    fn encode_metadata(
        &self,
        _metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn CodecContext,
    ) -> Vec<u8> {
        Vec::new()
    }

    fn decode_metadata(
        &self,
        _bytes: &[u8],
        _ctx: &dyn CodecContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        Arc::new(naviscope_api::models::graph::EmptyMetadata)
    }
}

impl PresentationCap for MockCap {
    fn naming_convention(&self) -> Option<Arc<dyn NamingConvention>> {
        Some(Arc::new(StandardNamingConvention))
    }

    fn node_presenter(&self) -> Option<Arc<dyn NodePresenter>> {
        Some(Arc::new(self.clone()))
    }

    fn symbol_kind(&self, _kind: &NodeKind) -> lsp_types::SymbolKind {
        lsp_types::SymbolKind::CLASS
    }
}

impl MetadataCodecCap for MockCap {
    fn metadata_codec(&self) -> Option<Arc<dyn NodeMetadataCodec>> {
        Some(Arc::new(self.clone()))
    }
}

fn mock_caps() -> LanguageCaps {
    let cap = Arc::new(MockCap);
    LanguageCaps {
        language: Language::new("mock"),
        matcher: cap.clone(),
        parser: cap.clone(),
        semantic: cap.clone() as Arc<dyn SemanticCap>,
        indexing: cap.clone(),
        asset: cap.clone(),
        presentation: cap.clone(),
        metadata_codec: cap,
    }
}

fn setup_engine(temp_dir: &Path) -> CoreEngine {
    ensure_test_index_dir();
    CoreEngine::builder(temp_dir.to_path_buf())
        .with_language_caps(mock_caps())
        .build()
}

fn ensure_test_index_dir() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join("naviscope_test_index_dir");
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("NAVISCOPE_INDEX_DIR", dir);
        }
    });
}

#[tokio::test]
async fn test_symbol_navigator_queries() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_navigator_real");
    std::fs::create_dir_all(&temp_dir).ok();

    let engine = setup_engine(&temp_dir);
    let test_file = temp_dir.join("test.mock");
    std::fs::write(&test_file, "class Symbol {}").unwrap();
    engine.update_files(vec![test_file.clone()]).await.unwrap();

    let handle = EngineHandle::from_engine(Arc::new(engine));
    let query = SymbolQuery {
        language: Language::new("mock"),
        resolution: SymbolResolution::Global("test::Symbol".to_string()),
    };

    let defs = handle.find_definitions(&query).await.unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].path.as_os_str(), test_file.as_os_str());
}

#[tokio::test]
async fn test_symbol_info_provider() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_info");
    std::fs::create_dir_all(&temp_dir).ok();

    let engine = setup_engine(&temp_dir);
    let handle = EngineHandle::from_engine(Arc::new(engine));

    let test_file = temp_dir.join("test.mock");
    std::fs::write(&test_file, "class Symbol {}").unwrap();
    let uri = format!("file://{}", test_file.display());

    let lang = handle.get_language_for_document(&uri).await.unwrap();
    assert_eq!(lang, Some(Language::new("mock")));

    let symbols = handle.get_document_symbols(&uri).await.unwrap();
    assert!(symbols.is_empty());
}
