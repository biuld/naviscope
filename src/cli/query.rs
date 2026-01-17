use naviscope::index::Naviscope;
use naviscope::query::GraphQuery;
use naviscope::query::QueryEngine;
use std::path::PathBuf;

pub fn run(path: PathBuf, query: String, index: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
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

    let query_obj: GraphQuery = serde_json::from_str(&query)?;
    let query_engine = QueryEngine::new(naviscope.index());
    let result = query_engine.execute(&query_obj)?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
