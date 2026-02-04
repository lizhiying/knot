use anyhow::Result;
use clap::{Parser, Subcommand};
use knot_core::{KnotIndexer, KnotStore};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "knot-cli")]
#[command(about = "CLI for Knot RAG Engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a directory
    Index {
        /// Path to the directory to index
        #[arg(short, long)]
        input: PathBuf,
    },
    /// Query the index
    Query {
        /// The query text
        #[arg(short, long)]
        text: String,
    },
    /// Watch a directory for changes
    Watch {
        /// Path to watch
        #[arg(short, long)]
        input: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = "knot_index.lance";

    match cli.command {
        Commands::Index { input } => {
            println!("Indexing directory: {:?}", input);
            // Use sqlite registry for incremental updates
            let indexer = KnotIndexer::new(Some("sqlite:knot.db?mode=rwc".to_string())).await;
            let (records, deleted_files) = indexer.index_directory(&input).await?;
            println!("Found {} vectors to add.", records.len());
            println!("Found {} files to delete.", deleted_files.len());

            let store = KnotStore::new(db_path).await?;

            // Handle deletions
            for del_path in deleted_files {
                println!("Deleting from store: {}", del_path);
                store.delete_file(&del_path).await?;
            }

            store.add_records(records).await?;
            store.create_fts_index().await?;
            println!("Indexing complete. Data saved to {}", db_path);
        }
        Commands::Query { text } => {
            println!("Querying: {}", text);
            let store = KnotStore::new(db_path).await?;

            // For now, use a mock embedding (random/zero) for query too
            // In reality this should match the embedding model used for indexing
            // In reality this should match the embedding model used for indexing
            let query_vec = vec![0.0; 384];

            let results = store.search(query_vec, &text).await?;
            println!("Found {} results:", results.len());
            for (i, res) in results.iter().enumerate() {
                println!("[{}] {} (Score: {:.4})", i + 1, res.file_path, res.score);
                if let Some(bc) = &res.breadcrumbs {
                    println!("    Context: {}", bc);
                }
                println!("    Sample: {}\n", res.text.lines().next().unwrap_or(""));
            }
        }
        Commands::Watch { input } => {
            use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
            use std::sync::mpsc::channel;

            println!("Watching directory: {:?}", input);
            let (tx, rx) = channel();

            let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
            watcher.watch(&input, RecursiveMode::Recursive)?;

            let indexer = KnotIndexer::new(Some("sqlite:knot.db?mode=rwc".to_string())).await;

            // Simple blocking loop for watch events
            // In a real app, this should handle async better.
            for res in rx {
                match res {
                    Ok(event) => {
                        let store = KnotStore::new(db_path).await?;
                        match event.kind {
                            notify::EventKind::Create(_) | notify::EventKind::Modify(_) => {
                                for path in event.paths {
                                    if should_process(&path) {
                                        println!("Change detected: {:?}", path);
                                        // For modify/create, we can just index this specific file
                                        // But index_file doesn't check registry hash?
                                        // index_directory checks hash.
                                        // We should ideally just re-index that file AND update registry.
                                        // KNotIndexer::index_file updates registry? No.
                                        // We need modification to index_file or manual registry update here?
                                        // indexer.registry is private.

                                        // Hack for MVP: Just call index_directory but point to file? No Walker fails.
                                        // Better: indexer.index_file returns records. We handle store add.
                                        // BUT registry update is missing!
                                        // Incremental logic relies on registry update.

                                        // Solution: Just run indexer.index_directory(&input).
                                        // It's fast because it skips unchanged files!
                                        // So "Watch" just triggers "Scan".
                                        println!("Scanning...");
                                        match indexer.index_directory(&input).await {
                                            Ok((records, deleted)) => {
                                                if !records.is_empty() {
                                                    println!("Adding {} records", records.len());
                                                    store.add_records(records).await?;
                                                }
                                                for del in deleted {
                                                    println!("Deleting {}", del);
                                                    store.delete_file(&del).await?;
                                                }
                                            }
                                            Err(e) => eprintln!("Index error: {}", e),
                                        }
                                    }
                                }
                            }
                            notify::EventKind::Remove(_) => {
                                // Re-scan to handle deletions robustly
                                println!("File removed. Scanning...");
                                match indexer.index_directory(&input).await {
                                    Ok((records, deleted)) => {
                                        if !records.is_empty() {
                                            store.add_records(records).await?;
                                        }
                                        for del in deleted {
                                            println!("Deleting {}", del);
                                            store.delete_file(&del).await?;
                                        }
                                    }
                                    Err(e) => eprintln!("Index error: {}", e),
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(e) => println!("watch error: {:?}", e),
                }
            }
        }
    }

    Ok(())
}

fn should_process(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext == "md" || ext == "txt"
    } else {
        false
    }
}
