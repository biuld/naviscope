use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, InitializeResult, ServerCapabilities},
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServiceExt,
};
use crate::query::GraphQuery;
use crate::index::Naviscope;
use crate::query::QueryEngine;
use crate::model::graph::EdgeType;
use std::path::PathBuf;
use serde::Deserialize;
use schemars::JsonSchema;

#[derive(Clone)]
pub struct NaviscopeMcp {
    tool_router: ToolRouter<Self>,
}

async fn execute_query(path: String, query: GraphQuery) -> Result<CallToolResult, McpError> {
    let result = tokio::task::spawn_blocking(move || {
        let mut naviscope = Naviscope::new(PathBuf::from(path));
        naviscope.build_index().map_err(|e| e.to_string())?;
        
        let engine = QueryEngine::new(naviscope.index());
        let result = engine.execute(&query).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
    }).await.map_err(|e| McpError::new(rmcp::model::ErrorCode(-32000), e.to_string(), None))?;

    match result {
        Ok(json_str) => Ok(CallToolResult::success(vec![Content::text(json_str)])),
        Err(e) => Err(McpError::new(rmcp::model::ErrorCode(-32000), e, None)),
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// The absolute path to the project root directory
    pub path: String,
    /// Search pattern (simple string or regex) for code element names
    pub pattern: String,
    /// Optional: Filter by element type (e.g., ["class", "method", "interface"])
    pub kind: Option<Vec<String>>,
    /// Maximum number of results to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LsArgs {
    /// The absolute path to the project root directory
    pub path: String,
    /// Target node FQN to list children for. If null, lists top-level modules.
    pub fqn: Option<String>,
    /// Optional: Filter results by element type (e.g., ["class", "method"])
    pub kind: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct InspectArgs {
    /// The absolute path to the project root directory
    pub path: String,
    /// The Fully Qualified Name (FQN) of the code element to inspect
    pub fqn: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EdgeArgs {
    /// The absolute path to the project root directory
    pub path: String,
    /// The Fully Qualified Name (FQN) of the target code element
    pub fqn: String,
    /// Optional: Filter by relationship types (e.g., ["Calls", "InheritsFrom"])
    pub edge_type: Option<Vec<EdgeType>>,
}

#[tool_router]
impl NaviscopeMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Search for code elements (classes, methods, fields, etc.) across the project using a name pattern or regex. Use this to find definitions when you only know a name or part of it.")]
    pub async fn grep(&self, params: Parameters<GrepArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        execute_query(args.path, GraphQuery::Grep { 
            pattern: args.pattern, 
            kind: args.kind.unwrap_or_default(), 
            limit: args.limit.unwrap_or(20) 
        }).await
    }

    #[tool(description = "List sub-elements of a given node (FQN) or list top-level project modules if FQN is omitted. Use this to explore package structures or class members.")]
    pub async fn ls(&self, params: Parameters<LsArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        execute_query(args.path, GraphQuery::Ls { 
            fqn: args.fqn, 
            kind: args.kind.unwrap_or_default() 
        }).await
    }

    #[tool(description = "Retrieve detailed information about a specific code element by its Fully Qualified Name (FQN), including its source code snippet, location, and metadata.")]
    pub async fn inspect(&self, params: Parameters<InspectArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        execute_query(args.path, GraphQuery::Inspect { fqn: args.fqn }).await
    }

    #[tool(description = "Find all code elements that depend on, call, or reference the specified FQN. Use this to perform impact analysis or find usages of a class/method.")]
    pub async fn incoming(&self, params: Parameters<EdgeArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        execute_query(args.path, GraphQuery::Incoming { 
            fqn: args.fqn, 
            edge_type: args.edge_type.unwrap_or_default() 
        }).await
    }

    #[tool(description = "Find all code elements that the specified FQN depends on, calls, or references. Use this to understand the dependencies or implementation details of a class/method.")]
    pub async fn outgoing(&self, params: Parameters<EdgeArgs>) -> Result<CallToolResult, McpError> {
        let args = params.0;
        execute_query(args.path, GraphQuery::Outgoing { 
            fqn: args.fqn, 
            edge_type: args.edge_type.unwrap_or_default() 
        }).await
    }
}

#[tool_handler]
impl rmcp::ServerHandler for NaviscopeMcp {
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

pub async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let service = NaviscopeMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
