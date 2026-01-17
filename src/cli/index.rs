use naviscope::index::Naviscope;
use std::path::PathBuf;

pub fn run(path: PathBuf, debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut naviscope = Naviscope::new(path.clone());
    println!("Indexing project at: {}...", path.display());
    naviscope.build_index()?;

    if debug {
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

    Ok(())
}
