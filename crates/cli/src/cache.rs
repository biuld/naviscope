use clap::Subcommand;
use tabled::{Table, Tabled};

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Show cache statistics
    Stats,
    /// List cached assets
    List {
        /// Sort by size or date
        #[arg(long, value_parser = ["size", "date"])]
        sort: Option<String>,
        /// Filter by path pattern
        #[arg(long)]
        filter: Option<String>,
    },
    /// Inspect a specific cached asset
    Inspect {
        /// Asset hash (full or prefix)
        hash: String,
    },
    /// Clear the cache
    Clear,
}

#[derive(Tabled)]
struct AssetRow {
    #[tabled(rename = "Hash")]
    hash: String,
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(rename = "Stubs")]
    stubs: usize,
    #[tabled(rename = "Ver")]
    version: u32,
    #[tabled(rename = "Age")]
    age: String,
}

pub async fn run(cmd: CacheCommands) -> Result<(), Box<dyn std::error::Error>> {
    // Cache is global, we don't need a full engine to access it
    let cache = naviscope_runtime::get_cache_manager();

    match cmd {
        CacheCommands::Stats => {
            let stats = cache.stats();
            println!("Cache Directory: {}", stats.cache_dir.display());
            println!("Total Assets:    {}", stats.total_assets);
            println!("Total Entries:   {}", stats.total_entries);
        }
        CacheCommands::List { sort, filter } => {
            let mut assets = cache.scan_assets();

            // Filter
            if let Some(pattern) = filter {
                assets.retain(|a| a.path.contains(&pattern));
            }

            // Sort
            if let Some(key) = sort {
                match key.as_str() {
                    "size" => assets.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes)),
                    "date" => assets.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
                    _ => {}
                }
            }

            let rows: Vec<AssetRow> = assets
                .into_iter()
                .map(|a| {
                    let bytes = a.size_bytes;
                    let size_str = if bytes < 1024 {
                        format!("{} B", bytes)
                    } else if bytes < 1024 * 1024 {
                        format!("{:.1} KB", bytes as f64 / 1024.0)
                    } else {
                        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
                    };

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let age = now.saturating_sub(a.created_at);
                    let age_str = if age < 60 {
                        format!("{}s ago", age)
                    } else if age < 3600 {
                        format!("{}m ago", age / 60)
                    } else if age < 86400 {
                        format!("{}h ago", age / 3600)
                    } else {
                        format!("{}d ago", age / 86400)
                    };

                    AssetRow {
                        hash: a.hash,
                        path: a.path,
                        size: size_str,
                        stubs: a.stub_count,
                        version: a.version,
                        age: age_str,
                    }
                })
                .collect();

            if rows.is_empty() {
                println!("No cached assets found.");
            } else {
                println!("{}", Table::new(rows));
            }
        }
        CacheCommands::Inspect { hash } => {
            if let Some(result) = cache.inspect_asset(&hash) {
                println!("Asset Summary:");
                println!("  Path:    {}", result.summary.path);
                println!("  Hash:    {}", result.summary.hash);
                println!("  Version: {}", result.summary.version);
                println!("  Stubs:   {}", result.summary.stub_count);

                println!("\nMetadata Distribution:");
                for (type_tag, count) in result.metadata_distribution {
                    println!("  {}: {}", type_tag, count);
                }

                println!("\nSample Entries:");
                for (i, entry) in result.sample_entries.iter().enumerate() {
                    println!("  {}. {}", i + 1, entry);
                }
            } else {
                println!("Asset not found with hash prefix: {}", hash);
            }
        }
        CacheCommands::Clear => {
            cache.clear()?;
            println!("Cache cleared successfully.");
        }
    }

    Ok(())
}
