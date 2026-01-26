use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, InitializeResult, ServerCapabilities},
    tool, tool_handler, tool_router,
    ErrorData as McpError,
};
use crate::query::GraphQuery;
use crate::index::Naviscope;
use crate::query::QueryEngine;
use crate::model::graph::{EdgeType, NodeKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::Deserialize;
use schemars::JsonSchema;
use xxhash_rust::xxh3::xxh3_64;

pub mod http;
pub mod stdio;
pub mod proxy;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SessionInfo {
    pub port: u16,
    pub pid: u32,
    pub root_path: PathBuf,
}

pub fn get_session_path(root_path: &Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let session_dir = Path::new(&home).join(".naviscope/sessions");
    let _ = std::fs::create_dir_all(&session_dir);
    
    let abs_path = root_path.canonicalize().unwrap_or_else(|_| root_path.to_path_buf());
    let hash = xxh3_64(abs_path.to_string_lossy().as_bytes());
    session_dir.join(format!("{:016x}.json", hash))
}

#[derive(Clone)]
pub struct McpServer {
    pub(crate) tool_router: ToolRouter<Self>,
    pub(crate) engine: Arc<RwLock<Option<Naviscope>>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// Search pattern (simple string or regex) for code element names
    pub pattern: String,
    /// Optional: Filter by element type.
    /// Available kinds:
    /// - Code: class, interface, enum, annotation, method, constructor, field, package
    /// - Build: module, dependency, task, plugin
    pub kind: Option<Vec<String>>,
    /// Maximum number of results to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LsArgs {
    /// Target node FQN to list children for. If null, lists top-level modules.
    pub fqn: Option<String>,
    /// Optional: Filter results by element type.
    /// Available kinds:
    /// - Code: class, interface, enum, annotation, method, constructor, field, package
    /// - Build: module, dependency, task, plugin
    pub kind: Option<Vec<String>>,
    /// Optional: Filter results by modifiers (e.g. ["public", "static"])
    pub modifiers: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct InspectArgs {
    /// The Fully Qualified Name (FQN) of the code element to inspect
    pub fqn: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct DepsArgs {
    /// The Fully Qualified Name (FQN) of the target code element
    pub fqn: String,
    /// If true, find incoming dependencies (who depends on me). 
    /// If false (default), find outgoing dependencies (who do I depend on).
    #[serde(default)]
    pub rev: bool,
    /// Optional: Filter by relationship types.
    /// Available types:
    /// - Structural: Contains
    /// - Inheritance: InheritsFrom, Implements
    /// - Usage: Calls, Instantiates, TypedAs, DecoratedBy, UsesDependency
    pub edge_type: Option<Vec<EdgeType>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetGuideArgs {}

#[tool_router]
impl McpServer {
    pub fn new(engine: Arc<RwLock<Option<Naviscope>>>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            engine,
        }
    }

    pub(crate) async fn get_or_build_index(&self) -> Result<Naviscope, McpError> {
        let lock = self.engine.read().await;
        
        match &*lock {
            Some(navi) => Ok(navi.clone()),
            None => {
                // Index not yet built by LSP, return error
                Err(McpError::new(
                    rmcp::model::ErrorCode(-32000),
                    "Index not yet available. The LSP server is still building the index. Please wait a moment and try again.".to_string(),
                    None
                ))
            }
        }
    }

    pub(crate) async fn execute_query(&self, query: GraphQuery) -> Result<CallToolResult, McpError> {
        let engine = self.get_or_build_index().await?;
        
        let result = tokio::task::spawn_blocking(move || {
            let query_engine = QueryEngine::new(engine.graph());
            let result = query_engine.execute(&query).map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
        }).await.map_err(|e| McpError::new(rmcp::model::ErrorCode(-32000), e.to_string(), None))?;

        match result {
            Ok(json_str) => Ok(CallToolResult::success(vec![Content::text(json_str)])),
            Err(e) => Err(McpError::new(rmcp::model::ErrorCode(-32000), e, None)),
        }
    }

    fn to_node_kinds(kinds: Option<Vec<String>>) -> Vec<NodeKind> {
        kinds.unwrap_or_default()
            .iter()
            .map(|s| NodeKind::from(s.as_str()))
            .collect()
    }

    #[tool(description = "Returns a comprehensive user guide and examples for using Naviscope. Call this tool first to understand how to effectively explore and analyze the codebase using the available tools.")]
    pub async fn get_guide(&self, _params: Parameters<GetGuideArgs>) -> Result<CallToolResult, McpError> {
        let guide = r#"
# Naviscope User Guide

Naviscope is a graph-based code understanding engine. Unlike text search, it understands the structural and semantic relationships in your code (Calls, Inheritance, Dependencies).

## 🚀 Recommended Workflow

1. **Explore Structure**: Use `ls` to visualize the project hierarchy (modules, packages).
   - `ls()` -> List root modules
   - `ls(fqn="com.example")` -> List contents of a package

2. **Find Entry Points**: Use `grep` to locate specific symbols (classes, methods) by name.
   - `grep(pattern="UserController", kind=["class"])`

3. **Deep Analysis**: Once you have a Fully Qualified Name (FQN), use `inspect` and `deps`.
   - `inspect(fqn="...")` -> View source code and metadata
   - `deps(fqn="...")` -> View outgoing calls/dependencies (What does this code use?)
   - `deps(fqn="...", rev=true)` -> View incoming calls (Who uses this code?)

## 💡 Tips
- **FQNs**: Naviscope relies on Fully Qualified Names (e.g., `com.example.MyClass`, `src/main.rs`). Always use the FQN returned by `ls` or `grep` for subsequent `inspect`/`deps` calls.
- **Filters**: Use the `kind` (e.g., "class", "method") and `edge_type` (e.g., "Calls", "InheritsFrom") filters to narrow down noisy results.
"#;
        Ok(CallToolResult::success(vec![Content::text(guide)]))
    }

    #[tool(description = "Search for code elements (classes, methods, fields, etc.) across the project using a name pattern or regex. Use this to find definitions when you only know a name or part of it.")]
    pub async fn grep(&self, params: Parameters<GrepArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Grep { 
            pattern: args.pattern, 
            kind: Self::to_node_kinds(args.kind), 
            limit: args.limit.unwrap_or(20) 
        }).await
    }

    #[tool(description = "List sub-elements of a given node (FQN) or list top-level project modules if FQN is omitted. Use this to explore package structures or class members.")]
    pub async fn ls(&self, params: Parameters<LsArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Ls { 
            fqn: args.fqn, 
            kind: Self::to_node_kinds(args.kind),
            modifiers: args.modifiers.unwrap_or_default(),
        }).await
    }

    #[tool(description = "Retrieve detailed information about a specific code element by its Fully Qualified Name (FQN), including its source code snippet, location, and metadata.")]
    pub async fn inspect(&self, params: Parameters<InspectArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Cat { fqn: args.fqn }).await
    }

    #[tool(description = "Analyze dependencies for a given FQN. By default, shows outgoing dependencies (who I depend on). Use rev=true for incoming dependencies (who depends on me/impact analysis).")]
    pub async fn deps(&self, params: Parameters<DepsArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Deps { 
            fqn: args.fqn, 
            rev: args.rev,
            edge_types: args.edge_type.unwrap_or_default() 
        }).await
    }
}

#[tool_handler]
impl rmcp::ServerHandler for McpServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: rmcp::model::ProtocolVersion::V_2024_11_05,
            server_info: Implementation {
                name: "naviscope".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}
