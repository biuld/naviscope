mod clear;
mod index;
mod shell;
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
    #[command(
        long_about = "Analyzes the project structure and source code to build a persistent index. \
                            By default, the index is stored in ~/.naviscope/indices/."
    )]
    Index {
        /// Path to the project root directory to index
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Save a human-readable JSON version for debugging purposes
        #[arg(long)]
        debug: bool,
    },
    /// Start an interactive shell to query the code knowledge graph
    #[command(
        long_about = "Starts an interactive shell where you can execute structured queries \
                            against the index using both JSON DSL and shorthand commands."
    )]
    Shell {
        /// Path to the project root (used to locate the default index). Defaults to current directory.
        #[arg(value_name = "PROJECT_PATH")]
        path: Option<PathBuf>,
    },
    /// Watch for file changes and update the index automatically
    #[command(
        long_about = "Starts a file watcher that monitors the project directory for changes. \
                            When a change is detected, the index is automatically updated."
    )]
    Watch {
        /// Path to the project root directory to watch
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Save a human-readable JSON version for debugging purposes
        #[arg(long)]
        debug: bool,
    },
    /// Clear built indices
    #[command(
        long_about = "Removes built index files. If a path is provided, only that project's index \
                            is removed. Otherwise, all indices are cleared."
    )]
    Clear {
        /// Path to the project root directory to clear (optional)
        #[arg(value_name = "PROJECT_PATH")]
        path: Option<PathBuf>,
    },
    /// Start the Model Context Protocol (MCP) server
    Mcp {
        /// Path to the project root directory
        #[arg(long, value_name = "PROJECT_PATH")]
        path: Option<PathBuf>,
    },
    /// Start the Language Server Protocol (LSP) server
    Lsp,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging based on command
    let component = match &cli.command {
        Commands::Lsp => "lsp",
        Commands::Mcp { .. } => "mcp",
        _ => "cli",
    };
    let _guard = naviscope::logging::init_logging(component);

    match cli.command {
        Commands::Index { path, debug } => index::run(path, debug),
        Commands::Shell { path } => shell::run(path),
        Commands::Watch { path, debug } => watch::run(path, debug),
        Commands::Clear { path } => clear::run(path),
        Commands::Mcp { path } => {
            let rt = tokio::runtime::Runtime::new()?;
            let project_path = path
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            // Connect to LSP via proxy mode (waits for LSP if not started)
            rt.block_on(async { naviscope::mcp::proxy::run_mcp_proxy(&project_path).await })?;
            Ok(())
        }
        Commands::Lsp => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async { naviscope::lsp::run_server().await })?;
            Ok(())
        }
    }
}
