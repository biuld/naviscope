use super::view::{ShellNodeView, ShellNodeViewShort, get_kind_weight};
use clap::{Parser, ValueEnum};
use naviscope_api::models::{EdgeType, GraphQuery, NodeKind, QueryResult};
use shlex;
use std::sync::Arc;
use tabled::{Table, settings::Style};

/// Default limit for search results
const DEFAULT_SEARCH_LIMIT: usize = 20;

#[derive(Clone, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum CliNodeKind {
    Class,
    Interface,
    Enum,
    Annotation,
    Method,
    Constructor,
    Field,
    Package,
    Project,
    Module,
    Dependency,
    Task,
    Plugin,
    Other,
}

impl From<CliNodeKind> for NodeKind {
    fn from(kind: CliNodeKind) -> Self {
        match kind {
            CliNodeKind::Class => NodeKind::Class,
            CliNodeKind::Interface => NodeKind::Interface,
            CliNodeKind::Enum => NodeKind::Enum,
            CliNodeKind::Annotation => NodeKind::Annotation,
            CliNodeKind::Method => NodeKind::Method,
            CliNodeKind::Constructor => NodeKind::Constructor,
            CliNodeKind::Field => NodeKind::Field,
            CliNodeKind::Package => NodeKind::Package,
            CliNodeKind::Project => NodeKind::Project,
            CliNodeKind::Module => NodeKind::Module,
            CliNodeKind::Dependency => NodeKind::Dependency,
            CliNodeKind::Task => NodeKind::Task,
            CliNodeKind::Plugin => NodeKind::Plugin,
            CliNodeKind::Other => NodeKind::Custom("other".to_string()),
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum CliEdgeType {
    Contains,
    InheritsFrom,
    Implements,
    TypedAs,
    DecoratedBy,
    UsesDependency,
}

impl From<CliEdgeType> for EdgeType {
    fn from(kind: CliEdgeType) -> Self {
        match kind {
            CliEdgeType::Contains => EdgeType::Contains,
            CliEdgeType::InheritsFrom => EdgeType::InheritsFrom,
            CliEdgeType::Implements => EdgeType::Implements,
            CliEdgeType::TypedAs => EdgeType::TypedAs,
            CliEdgeType::DecoratedBy => EdgeType::DecoratedBy,
            CliEdgeType::UsesDependency => EdgeType::UsesDependency,
        }
    }
}

/// Helper struct for Clap parsing within the shell
#[derive(Parser, Clone)]
#[command(no_binary_name = true)]
pub enum ShellCommand {
    /// List members or structure
    Ls {
        /// Target node FQN (optional, defaults to current node)
        fqn: Option<String>,
        /// Filter by node kind (e.g. class, interface, method)
        #[arg(long, value_delimiter = ',')]
        kind: Vec<CliNodeKind>,
        /// Filter by modifiers (e.g. public, static)
        #[arg(long, value_delimiter = ',')]
        modifiers: Vec<String>,
        /// Use long listing format
        #[arg(short, long)]
        long: bool,
    },
    /// Change current node context (internal shell command)
    Cd {
        /// Target path
        path: String,
    },
    /// Print current node context
    Pwd,
    /// Clear the screen
    Clear,
    /// Search for symbols
    Find {
        /// Pattern to search for
        pattern: String,
        /// Filter by node kind
        #[arg(long, value_delimiter = ',')]
        kind: Vec<CliNodeKind>,
        /// Limit number of results
        #[arg(long, default_value_t = DEFAULT_SEARCH_LIMIT)]
        limit: usize,
    },
    /// Inspect node details
    Cat {
        /// Target node FQN (optional, defaults to current node) or member name
        target: String,
    },
    /// Find dependencies
    Deps {
        /// Target node FQN (optional, defaults to current node)
        fqn: Option<String>,
        /// If set, find incoming dependencies (who depends on me)
        #[arg(long)]
        rev: bool,
        /// Filter by edge types (e.g. TypedAs, InheritsFrom)
        #[arg(long, value_delimiter = ',')]
        edge_types: Vec<CliEdgeType>,
    },
}

use clap::error::ErrorKind;

impl ShellCommand {
    /// Automatically generates the list of available command names from the enum.
    /// This eliminates the need to manually maintain a hardcoded command list.
    pub fn command_names() -> Vec<String> {
        use clap::CommandFactory;
        let cmd = Self::command();
        let mut names = vec!["help".to_string(), "exit".to_string(), "quit".to_string()];
        names.extend(cmd.get_subcommands().map(|s| s.get_name().to_string()));
        names
    }
}

pub fn parse_shell_command(
    input: &str,
) -> Result<Option<ShellCommand>, Box<dyn std::error::Error>> {
    // Use shlex to split arguments while respecting quotes
    let args = shlex::split(input).ok_or("Invalid quoting")?;

    // Parse using Clap
    match ShellCommand::try_parse_from(args) {
        Ok(c) => Ok(Some(c)),
        Err(e) => {
            // Handle help/version display without returning an error
            if e.kind() == ErrorKind::DisplayHelp || e.kind() == ErrorKind::DisplayVersion {
                println!("{}", e);
                return Ok(None);
            }
            Err(Box::new(e))
        }
    }
}

impl ShellCommand {
    pub fn to_graph_query(
        &self,
        current_node: &Option<String>,
    ) -> Result<GraphQuery, Box<dyn std::error::Error>> {
        match self {
            ShellCommand::Ls {
                fqn,
                kind,
                modifiers,
                ..
            } => {
                let target_fqn = fqn.clone().or_else(|| current_node.clone());
                Ok(GraphQuery::Ls {
                    fqn: target_fqn,
                    kind: kind.iter().map(|k| k.clone().into()).collect(),
                    modifiers: modifiers.clone(),
                })
            }
            ShellCommand::Find {
                pattern,
                kind,
                limit,
            } => Ok(GraphQuery::Find {
                pattern: pattern.clone(),
                kind: kind.iter().map(|k| k.clone().into()).collect(),
                limit: *limit,
            }),
            ShellCommand::Cat { target } => Ok(GraphQuery::Cat {
                fqn: target.clone(),
            }),
            ShellCommand::Deps {
                fqn,
                rev,
                edge_types,
            } => {
                let target_fqn = fqn
                    .clone()
                    .or_else(|| current_node.clone())
                    .ok_or("No FQN provided and no current context")?;
                Ok(GraphQuery::Deps {
                    fqn: target_fqn,
                    rev: *rev,
                    edge_types: edge_types.iter().map(|e| e.clone().into()).collect(),
                })
            }
            ShellCommand::Cd { .. } | ShellCommand::Pwd | ShellCommand::Clear => {
                Err("Internal shell command should be handled by ReplServer".into())
            }
        }
    }

    pub fn render(
        &self,
        result: QueryResult,
        context: &super::context::ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if result.nodes.is_empty() {
            return Ok("NO RECORDS FOUND".to_string());
        }

        match self {
            ShellCommand::Ls { long: false, .. } => {
                let mut views: Vec<ShellNodeViewShort> = result
                    .nodes
                    .iter()
                    .map(|node| ShellNodeViewShort {
                        kind: node.kind.to_string(),
                        name: if is_container(node.kind.clone()) {
                            format!("{}/", node.name)
                        } else {
                            node.name.to_string()
                        },
                    })
                    .collect();

                views.sort_by(|a, b| {
                    let wa = get_kind_weight(&a.kind);
                    let wb = get_kind_weight(&b.kind);
                    if wa != wb {
                        wa.cmp(&wb)
                    } else {
                        a.name.cmp(&b.name)
                    }
                });

                Ok(Table::new(&views).with(Style::psql()).to_string())
            }
            ShellCommand::Cat { .. } if result.nodes.len() == 1 => {
                Ok(serde_json::to_string_pretty(&result.nodes[0])?)
            }
            _ => {
                // Default detailed table view for Find, Deps, and Ls -l
                let mut views: Vec<ShellNodeView> = result
                    .nodes
                    .iter()
                    .map(|node| {
                        let relation = result
                            .edges
                            .iter()
                            .filter(|e| e.to == node.id || e.from == node.id)
                            .map(|e| format!("{:?}", e.data.edge_type))
                            .collect::<Vec<_>>()
                            .join(", ");

                        // Get feature provider based on node's language
                        use naviscope_api::models::Language;
                        let lang = match node.lang.as_ref() {
                            "java" => Language::Java,
                            _ => Language::BuildFile, // Default fallback
                        };

                        let feature_provider =
                            context.get_feature_provider(lang).unwrap_or_else(|| {
                                // Create a dummy feature provider that returns None for everything
                                use naviscope_api::plugin::LanguageFeatureProvider;
                                struct DummyProvider;
                                impl LanguageFeatureProvider for DummyProvider {
                                    fn detail_view(
                                        &self,
                                        _node: &naviscope_api::models::GraphNode,
                                    ) -> Option<String> {
                                        None
                                    }
                                    fn signature(
                                        &self,
                                        _node: &naviscope_api::models::GraphNode,
                                    ) -> Option<String> {
                                        None
                                    }
                                    fn modifiers(
                                        &self,
                                        _node: &naviscope_api::models::GraphNode,
                                    ) -> Vec<String> {
                                        vec![]
                                    }
                                }
                                Arc::new(DummyProvider)
                            });

                        ShellNodeView::from_node(
                            node,
                            if relation.is_empty() {
                                None
                            } else {
                                Some(relation)
                            },
                            &feature_provider,
                        )
                    })
                    .collect();

                views.sort_by(|a, b| {
                    let wa = get_kind_weight(&a.kind);
                    let wb = get_kind_weight(&b.kind);
                    if wa != wb {
                        wa.cmp(&wb)
                    } else {
                        a.name.cmp(&b.name)
                    }
                });

                Ok(Table::new(&views).with(Style::psql()).to_string())
            }
        }
    }
}

fn is_container(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Interface
            | NodeKind::Enum
            | NodeKind::Annotation
            | NodeKind::Module
            | NodeKind::Package
    )
}
