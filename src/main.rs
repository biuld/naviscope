use clap::{Parser, Subcommand};
use naviscope::index::Naviscope;
use naviscope::query::GraphQuery;
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
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a project directory into a Code Knowledge Graph
    #[command(long_about = "Analyzes the project structure and source code to build a persistent index. \
                            By default, the index is stored in ~/.naviscope/indices/.")]
    Index {
        /// Path to the project root directory to index
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Optional: Specific output path for the index file (.bin)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

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

        /// Optional: Direct path to a specific index file (overrides automatic lookup)
        #[arg(short, long, value_name = "INDEX_FILE")]
        index: Option<PathBuf>,
    },
    /// Show the JSON schema/examples for GraphQuery
    Schema,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Index {
            path,
            output,
            debug,
        } => {
            let mut naviscope = Naviscope::new(path.clone());
            println!("Indexing project at: {}...", path.display());
            naviscope.build_index()?;

            if let Some(final_output) = output {
                println!("Exporting index to: {}...", final_output.display());
                naviscope.save_to_file(&final_output)?;
            }

            if *debug {
                let json_path = PathBuf::from("naviscope_debug.json");
                println!("Debug mode: saving JSON index to: {}...", json_path.display());
                naviscope.save_to_json(json_path)?;
            }

            let index = naviscope.index();
            println!("Indexing complete!");
            println!("Nodes: {}", index.graph.node_count());
            println!("Edges: {}", index.graph.edge_count());

            println!("\nTop 10 nodes:");
            for (fqn, _) in index.fqn_map.iter().take(10) {
                println!(" - {}", fqn);
            }
        }
        Commands::Query { path, query, index } => {
            let naviscope = if let Some(index_file) = index {
                Naviscope::load_from_file(index_file)?
            } else {
                let mut ns = Naviscope::new(path.clone());
                ns.load()?;
                if ns.index().graph.node_count() == 0 {
                    println!(
                        "No index found for project at {}. Auto-indexing now...",
                        path.display()
                    );
                    ns.build_index()?;
                }
                ns
            };

            let query_obj: GraphQuery = serde_json::from_str(query)?;

            use naviscope::query::QueryEngine;
            let query_engine = QueryEngine::new(naviscope.index());
            let result = query_engine.execute(&query_obj)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Schema => {
            println!("GraphQuery JSON Interface Specification:");
            println!("======================================");
            
            println!("\nAvailable Node Kinds (for kind filters):");
            println!("  - Java: class, interface, enum, annotation, method, field");
            println!("  - Build: package (module), dependency");

            println!("\nAvailable Edge Types (for edge_type filters):");
            println!("  Contains, InheritsFrom, Implements, Calls, References, Instantiates, UsesDependency");

            println!("\n1. GREP - Search for symbols");
            let grep = serde_json::json!({
                "command": "grep",
                "pattern": "String (Required) - Regex or plain text",
                "kind": "Array of Strings (Optional, Default: []) - e.g., ['class', 'method']",
                "limit": "Number (Optional, Default: 20)"
            });
            println!("{}", serde_json::to_string_pretty(&grep)?);

            println!("\n2. LS - List members of a node or project");
            let ls = serde_json::json!({
                "command": "ls",
                "fqn": "String (Optional, Default: null) - Full path of node to list",
                "kind": "Array of Strings (Optional, Default: []) - Filter by node kind"
            });
            println!("{}", serde_json::to_string_pretty(&ls)?);

            println!("\n3. INSPECT - Get full details of a node");
            let inspect = serde_json::json!({
                "command": "inspect",
                "fqn": "String (Required) - Target node FQN"
            });
            println!("{}", serde_json::to_string_pretty(&inspect)?);

            println!("\n4. INCOMING / OUTGOING - Trace relationships");
            let relations = serde_json::json!({
                "command": "incoming | outgoing",
                "fqn": "String (Required) - Target node FQN",
                "edge_type": "Array of EdgeTypes (Optional, Default: []) - Filter by relationship type"
            });
            println!("{}", serde_json::to_string_pretty(&relations)?);
        }
    }

    Ok(())
}
