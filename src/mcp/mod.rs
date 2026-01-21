use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, InitializeResult, ServerCapabilities},
    tool, tool_handler, tool_router,
    ErrorData as McpError,
};
use crate::query::GraphQuery;
use crate::index::Naviscope;
use crate::query::QueryEngine;
use crate::model::graph::EdgeType;
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
    pub(crate) root_path: Option<PathBuf>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// Search pattern (simple string or regex) for code element names
    pub pattern: String,
    /// Optional: Filter by element type (e.g., ["class", "method", "interface"])
    pub kind: Option<Vec<String>>,
    /// Maximum number of results to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LsArgs {
    /// Target node FQN to list children for. If null, lists top-level modules.
    pub fqn: Option<String>,
    /// Optional: Filter results by element type (e.g., ["class", "method"])
    pub kind: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct InspectArgs {
    /// The Fully Qualified Name (FQN) of the code element to inspect
    pub fqn: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EdgeArgs {
    /// The Fully Qualified Name (FQN) of the target code element
    pub fqn: String,
    /// Optional: Filter by relationship types (e.g., ["Calls", "InheritsFrom"])
    pub edge_type: Option<Vec<EdgeType>>,
}

#[tool_router]
impl McpServer {
    pub fn new(engine: Arc<RwLock<Option<Naviscope>>>, root_path: Option<PathBuf>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            engine,
            root_path,
        }
    }

    pub(crate) async fn get_or_build_index(&self) -> Result<Naviscope, McpError> {
        let mut lock = self.engine.write().await;
        
        if let Some(navi) = &*lock {
            return Ok(navi.clone());
        }

        // Standalone mode: build index if not present
        let path = self.root_path.clone().unwrap_or_else(|| PathBuf::from("."));
        let mut navi = Naviscope::new(path);
        
        let (res, n) = tokio::task::spawn_blocking(move || {
            let res = navi.build_index();
            (res, navi)
        }).await.map_err(|e| McpError::new(rmcp::model::ErrorCode(-32000), e.to_string(), None))?;

        res.map_err(|e| McpError::new(rmcp::model::ErrorCode(-32000), e.to_string(), None))?;
        
        *lock = Some(n.clone());
        Ok(n)
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

    #[tool(description = "Search for code elements (classes, methods, fields, etc.) across the project using a name pattern or regex. Use this to find definitions when you only know a name or part of it.")]
    pub async fn grep(&self, params: Parameters<GrepArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Grep { 
            pattern: args.pattern, 
            kind: args.kind.unwrap_or_default(), 
            limit: args.limit.unwrap_or(20) 
        }).await
    }

    #[tool(description = "List sub-elements of a given node (FQN) or list top-level project modules if FQN is omitted. Use this to explore package structures or class members.")]
    pub async fn ls(&self, params: Parameters<LsArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Ls { 
            fqn: args.fqn, 
            kind: args.kind.unwrap_or_default() 
        }).await
    }

    #[tool(description = "Retrieve detailed information about a specific code element by its Fully Qualified Name (FQN), including its source code snippet, location, and metadata.")]
    pub async fn inspect(&self, params: Parameters<InspectArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Inspect { fqn: args.fqn }).await
    }

    #[tool(description = "Find all code elements that depend on, call, or reference the specified FQN. Use this to perform impact analysis or find usages of a class/method.")]
    pub async fn incoming(&self, params: Parameters<EdgeArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Incoming { 
            fqn: args.fqn, 
            edge_type: args.edge_type.unwrap_or_default() 
        }).await
    }

    #[tool(description = "Find all code elements that the specified FQN depends on, calls, or references. Use this to understand the dependencies or implementation details of a class/method.")]
    pub async fn outgoing(&self, params: Parameters<EdgeArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        self.execute_query(GraphQuery::Outgoing { 
            fqn: args.fqn, 
            edge_type: args.edge_type.unwrap_or_default() 
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
