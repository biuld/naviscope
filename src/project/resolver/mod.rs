pub mod strategy;

use crate::error::Result;
use crate::model::graph::{GraphEdge, GraphNode};
use crate::project::scanner::ParsedFile;
use crate::project::source::{BuildTool, Language};
use strategy::{BuildResolver, LangResolver, ProjectContext};
use strategy::gradle::GradleResolver;
use strategy::java::JavaResolver;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use rayon::prelude::*;

/// Graph operation commands that can be computed in parallel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphOp {
    /// Add or update a node
    AddNode { id: String, data: GraphNode },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        from_id: String,
        to_id: String,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath { path: PathBuf },
}

/// Result of resolving a single file
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this file into the graph
    pub ops: Vec<GraphOp>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn add_node(&mut self, id: String, data: GraphNode) {
        self.ops.push(GraphOp::AddNode { id, data });
    }

    pub fn add_edge(&mut self, from_id: String, to_id: String, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
    }
}

/// Main resolver that dispatches to specific strategies based on file type
pub struct Resolver {
    build_strategies: HashMap<BuildTool, Box<dyn BuildResolver>>,
    lang_strategies: HashMap<Language, Box<dyn LangResolver>>,
}

impl Resolver {
    pub fn new() -> Self {
        let mut build_strategies: HashMap<BuildTool, Box<dyn BuildResolver>> = HashMap::new();
        let mut lang_strategies: HashMap<Language, Box<dyn LangResolver>> = HashMap::new();

        // Register build strategies
        build_strategies.insert(BuildTool::Gradle, Box::new(GradleResolver::new()));

        // Register language strategies
        lang_strategies.insert(Language::Java, Box::new(JavaResolver::new()));

        Self {
            build_strategies,
            lang_strategies,
        }
    }

    /// Resolve all parsed files into graph operations using a two-phase process
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();

        // Add RemovePath operations for each file being processed to ensure a clean state
        for file in &files {
            all_ops.push(GraphOp::RemovePath {
                path: file.file.path.clone(),
            });
        }

        // Separate files into build and source files
        let (build_files, source_files): (Vec<_>, Vec<_>) =
            files.into_iter().partition(|f| f.is_build());

        // Phase 1: Resolve Build Tools (Structure)
        let mut project_context = ProjectContext::new();
        
        // Group build files by tool
        let mut builds_by_tool: HashMap<BuildTool, Vec<&ParsedFile>> = HashMap::new();
        for f in &build_files {
            if let Some(tool) = f.build_tool() {
                builds_by_tool.entry(tool).or_default().push(f);
            }
        }

        for (tool, tool_files) in builds_by_tool {
            if let Some(strategy) = self.build_strategies.get(&tool) {
                let (unit, context) = strategy.resolve(&tool_files)?;
                all_ops.extend(unit.ops);
                // Merge context
                project_context.path_to_module.extend(context.path_to_module);
            }
        }

        // Phase 2: Resolve Source Files (Entities) in parallel
        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .par_iter()
            .map(|file| {
                let language = file.language().unwrap_or(Language::BuildFile);

                if let Some(strategy) = self.lang_strategies.get(&language) {
                    strategy.resolve(file, &project_context)
                } else {
                    Ok(ResolvedUnit::new())
                }
            })
            .collect();

        // Collect and merge source operations
        for result in source_results {
            all_ops.extend(result?.ops);
        }

        Ok(all_ops)
    }
}
