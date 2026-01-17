mod index;
mod query;
mod schema;
mod watch;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "naviscope",
    version,
    about = "A graph-based structured code query engine for LLMs",
    long_about = "Naviscope builds a comprehensive Code Knowledge Graph by analyzing source code semantics \
                  and project structures. It provides a structured query interface optimized for LLM agents \
                  to explore and reason about complex codebases."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Index a project directory into a Code Knowledge Graph
    #[command(long_about = "Analyzes the project structure and source code to build a persistent index. \
                            By default, the index is stored in ~/.naviscope/indices/.")]
    Index {
        /// Path to the project root directory to index
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Save a human-readable JSON version for debugging purposes
        #[arg(long)]
        debug: bool,
    },
    /// Query the code knowledge graph using JSON DSL
    #[command(long_about = "Executes a structured query against an existing index. \
                            The query should be a JSON object following the GraphQuery schema.")]
    Query {
        /// Path to the project root (used to locate the default index)
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Structured query in JSON format (e.g., '{"command": "grep", "pattern": "MyClass"}')
        #[arg(value_name = "JSON_QUERY")]
        query: String,
    },
    /// Show the JSON schema/examples for GraphQuery
    Schema,
    /// Watch for file changes and update the index automatically
    #[command(long_about = "Starts a file watcher that monitors the project directory for changes. \
                            When a change is detected, the index is automatically updated.")]
    Watch {
        /// Path to the project root directory to watch
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Save a human-readable JSON version for debugging purposes
        #[arg(long)]
        debug: bool,
    },
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            path,
            debug,
        } => index::run(path, debug),
        Commands::Query { path, query } => query::run(path, query),
        Commands::Schema => schema::run(),
        Commands::Watch { path, debug } => watch::run(path, debug),
    }
}
