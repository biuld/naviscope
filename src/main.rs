use clap::{Parser, Subcommand};
use naviscope::index::Naviscope;
use naviscope::query::GraphQuery;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a project directory
    Index {
        /// Path to the project root
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Output path for the index file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Save a human-readable JSON version for debugging
        #[arg(long)]
        debug: bool,
    },
    /// Query the code knowledge graph
    Query {
        /// Path to the project root
        #[arg(value_name = "PROJECT_PATH")]
        path: PathBuf,

        /// Query JSON string
        #[arg(value_name = "JSON_QUERY")]
        query: String,

        /// Direct path to a specific index file (overrides project path lookup)
        #[arg(short, long)]
        index: Option<PathBuf>,
    },
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
    }

    Ok(())
}
