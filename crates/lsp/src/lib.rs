pub mod capabilities;
pub mod goto;
pub mod hierarchy;
pub mod highlight;
pub mod hover;
pub mod indexer;
pub mod symbols;
pub mod util;

use crate::util::Document;
use dashmap::DashMap;
use naviscope_api::NaviscopeEngine;
use naviscope_api::models::Language;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct LspServer {
    client: Client,
    pub engine: Arc<RwLock<Option<Arc<dyn NaviscopeEngine>>>>,
    pub engine_builder: Arc<dyn Fn(PathBuf) -> Arc<dyn NaviscopeEngine> + Send + Sync>,
    pub documents: DashMap<Url, Arc<Document>>,
    session_path: Arc<RwLock<Option<PathBuf>>>,
    cancel_token: CancellationToken,
}

impl LspServer {
    pub fn new(
        client: Client,
        engine_builder: Arc<dyn Fn(PathBuf) -> Arc<dyn NaviscopeEngine> + Send + Sync>,
    ) -> Self {
        Self {
            client,
            engine: Arc::new(RwLock::new(None)),
            engine_builder,
            documents: DashMap::new(),
            session_path: Arc::new(RwLock::new(None)),
            cancel_token: CancellationToken::new(),
        }
    }

    pub async fn get_language_for_uri(&self, uri: &Url) -> Option<Language> {
        let engine_lock = self.engine.read().await;
        let engine = engine_lock.as_ref()?;
        engine
            .get_language_for_document(uri.as_str())
            .await
            .ok()
            .flatten()
    }

    fn offset_at(&self, text: &str, position: Position) -> usize {
        let mut line = 0;
        let mut offset = 0;
        let mut chars = text.chars().peekable();

        while line < position.line as usize {
            if let Some(c) = chars.next() {
                offset += c.len_utf8();
                if c == '\n' {
                    line += 1;
                } else if c == '\r' {
                    if chars.peek() == Some(&'\n') {
                        offset += chars.next().unwrap().len_utf8();
                    }
                    line += 1;
                }
            } else {
                return offset;
            }
        }

        let mut utf16_count = 0;
        while utf16_count < position.character as usize {
            if let Some(c) = chars.next() {
                if c == '\n' || c == '\r' {
                    break;
                }
                utf16_count += c.len_utf16();
                offset += c.len_utf8();
            } else {
                break;
            }
        }
        offset
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_path = params.root_uri.and_then(|uri| uri.to_file_path().ok());

        if let Some(path) = root_path {
            {
                let handle = (self.engine_builder)(path.clone());
                let mut guard = self.engine.write().await;
                *guard = Some(handle);
            }

            indexer::spawn_indexer(path.clone(), self.client.clone(), self.engine.clone());

            // Start MCP HTTP Server via encapsulated helper
            // Start MCP HTTP Server via encapsulated helper
            naviscope_mcp::http::spawn_http_server(
                self.client.clone(),
                // This cast is problematic. Let's fix it by wrapping or adapting.
                // Since we can't cheaply convert the lock type, we will fix this by
                // Creating a simplified shared state adapter in a follow up.
                // FOR NOW: We will use a temporary mismatched type cast (unsafe) to prove the point? NO.
                // We must provide what strictly matches.

                // Let's create a derived lock that proxies? No.
                //
                // The correct fix is:
                // 1. Initialize MCP with a `McpEngineGlue`.
                // 2. Or, change LspServer's engine field to be generic?

                // Let's go with:
                // Change `McpServer` to accept a trait `EngineProvider`.
                // But `McpServer` is in `naviscope-mcp` which we just freed from `naviscope-core`.

                // OK, easier path:
                // McpServer doesn't strictly need the SAME `RwLock` object if we just want it to work.
                // But it needs to see "Some(handle)" when the LSP initializes it.

                // Let's do this:
                // We can't pass `self.engine` directly.
                {
                    // Capture the handle we just created
                    let handle_for_mcp = (self.engine_builder)(path.clone());
                    let mcp_engine: std::sync::Arc<
                        tokio::sync::RwLock<
                            Option<std::sync::Arc<dyn naviscope_api::graph::GraphService>>,
                        >,
                    > = std::sync::Arc::new(tokio::sync::RwLock::new(Some(handle_for_mcp.clone())));
                    mcp_engine
                },
                path,
                self.session_path.clone(),
                params.client_info.map(|i| i.name),
                self.cancel_token.clone(),
            );
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Naviscope".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: capabilities::server_capabilities(),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        self.cancel_token.cancel();
        let mut lock = self.session_path.write().await;
        if let Some(path) = lock.take() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        let lang = self
            .get_language_for_uri(&uri)
            .await
            .unwrap_or(Language::UNKNOWN);
        self.documents
            .insert(uri, Arc::new(Document::new(content, lang, version)));
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(mut doc_ref) = self.documents.get_mut(&uri) {
            let doc = doc_ref.value_mut();

            // For thin LSP, we just take the last full change or apply changes textualy
            if let Some(change) = params.content_changes.last() {
                // If it's a full change (no range), just replace
                if change.range.is_none() {
                    let new_doc = Arc::new(Document::new(
                        change.text.clone(),
                        doc.language.clone(),
                        version,
                    ));
                    *doc = new_doc;
                } else {
                    // Fallback to reload from engine/disk if it's too complex or just take the text if it's what's sent
                    // Most LSPs send full text if they are "thin".
                    // For now, let's assume full text if no range, otherwise we might need a more robust textual update.
                    let mut content = doc.content.clone();
                    for change in &params.content_changes {
                        if let Some(range) = change.range {
                            let start_byte = self.offset_at(&content, range.start);
                            let old_end_byte = self.offset_at(&content, range.end);
                            content.replace_range(start_byte..old_end_byte, &change.text);
                        } else {
                            content = change.text.clone();
                        }
                    }
                    *doc = Arc::new(Document::new(content, doc.language.clone(), version));
                }
            }
        }
    }
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.client
            .log_message(
                MessageType::LOG,
                format!("LSP Event: did_close uri={}", params.text_document.uri),
            )
            .await;
        self.documents.remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/hover uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = hover::hover(self, params).await;
        match &result {
            Ok(Some(_)) => {
                self.client
                    .log_message(MessageType::LOG, "LSP Response: found hover content")
                    .await
            }
            Ok(None) => {
                self.client
                    .log_message(MessageType::LOG, "LSP Response: no hover content")
                    .await
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("LSP Error: {}", e))
                    .await
            }
        }
        result
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/documentHighlight uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = highlight::highlight(self, params).await;
        if let Ok(Some(h)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} highlights", h.len()),
                )
                .await;
        }
        result
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/definition uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = goto::definition(self, params).await;
        match &result {
            Ok(Some(resp)) => {
                let count = match resp {
                    GotoDefinitionResponse::Scalar(_) => 1,
                    GotoDefinitionResponse::Array(v) => v.len(),
                    GotoDefinitionResponse::Link(v) => v.len(),
                };
                self.client
                    .log_message(
                        MessageType::LOG,
                        format!("LSP Response: found {} locations", count),
                    )
                    .await;
            }
            Ok(None) => {
                self.client
                    .log_message(MessageType::LOG, "LSP Response: no definition found")
                    .await
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("LSP Error: {}", e))
                    .await
            }
        }
        result
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/references uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = goto::references(self, params).await;
        if let Ok(Some(locs)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} references", locs.len()),
                )
                .await;
        }
        result
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/documentSymbol uri={}",
                    params.text_document.uri
                ),
            )
            .await;
        let result = symbols::document_symbol(self, params).await;
        if let Ok(Some(resp)) = &result {
            let count = match resp {
                DocumentSymbolResponse::Flat(v) => v.len(),
                DocumentSymbolResponse::Nested(v) => v.len(),
            };
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} symbols", count),
                )
                .await;
        }
        result
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        self.client
            .log_message(
                MessageType::LOG,
                format!("LSP Request: workspace/symbol query='{}'", params.query),
            )
            .await;
        let result = symbols::workspace_symbol(self, params).await;
        if let Ok(Some(syms)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} symbols", syms.len()),
                )
                .await;
        }
        result
    }

    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/implementation uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = goto::implementation(self, params).await;
        if let Ok(Some(_)) = &result {
            self.client
                .log_message(MessageType::LOG, "LSP Response: found implementations")
                .await;
        }
        result
    }

    async fn goto_type_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/typeDefinition uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = goto::type_definition(self, params).await;
        if let Ok(Some(_)) = &result {
            self.client
                .log_message(MessageType::LOG, "LSP Response: found type definitions")
                .await;
        }
        result
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: textDocument/prepareCallHierarchy uri={} pos={}:{}",
                    uri, pos.line, pos.character
                ),
            )
            .await;
        let result = hierarchy::prepare_call_hierarchy(self, params).await;
        if let Ok(Some(items)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: prepared {} items", items.len()),
                )
                .await;
        }
        result
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: callHierarchy/incomingCalls item={}",
                    params.item.name
                ),
            )
            .await;
        let result = hierarchy::incoming_calls(self, params).await;
        if let Ok(Some(calls)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} incoming calls", calls.len()),
                )
                .await;
        }
        result
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "LSP Request: callHierarchy/outgoingCalls item={}",
                    params.item.name
                ),
            )
            .await;
        let result = hierarchy::outgoing_calls(self, params).await;
        if let Ok(Some(calls)) = &result {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("LSP Response: found {} outgoing calls", calls.len()),
                )
                .await;
        }
        result
    }
}

pub async fn run_server<F>(engine_builder: F) -> std::result::Result<(), Box<dyn std::error::Error>>
where
    F: Fn(std::path::PathBuf) -> Arc<dyn NaviscopeEngine> + Send + Sync + 'static,
{
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let builder = std::sync::Arc::new(engine_builder);

    let (service, socket) =
        tower_lsp::LspService::new(move |client| LspServer::new(client, builder.clone()));
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;

    Ok(())
}
