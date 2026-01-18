use naviscope::index::Naviscope;
use std::path::PathBuf;

pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = path {
        let naviscope = Naviscope::new(path.clone());
        println!("Clearing index for project at: {}...", path.display());
        naviscope.clear_project_index()?;
        println!("Project index cleared.");
    } else {
        println!("Clearing all indices at: {}...", Naviscope::get_base_index_dir().display());
        Naviscope::clear_all_indices()?;
        println!("All indices cleared.");
    }
    Ok(())
}
