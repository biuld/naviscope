use naviscope::index::Naviscope;
use naviscope::query::GraphQuery;
use naviscope::query::QueryEngine;
use std::path::PathBuf;

pub fn run(path: PathBuf, query: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Naviscope::new(path);

    // Always perform incremental indexing before querying
    engine.build_index()?;

    let query_obj: GraphQuery = serde_json::from_str(&query)?;
    let query_engine = QueryEngine::new(engine.graph());
    let result = query_engine.execute(&query_obj)?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
