use anyhow::Result;
use clap::{Parser, Subcommand};
use knot_core::{KnotIndexer, KnotStore};
use std::path::PathBuf;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = "knot_index.lance";

    match cli.command {
        Commands::Index { input } => {
            println!("Indexing directory: {:?}", input);
            let indexer = KnotIndexer::new();
            let records = indexer.index_directory(&input).await?;
            println!("Found {} vectors.", records.len());

            let store = KnotStore::new(db_path).await?;
            store.add_records(records).await?;
            println!("Indexing complete. Data saved to {}", db_path);
        }
        Commands::Query { text } => {
            println!("Querying: {}", text);
            let store = KnotStore::new(db_path).await?;

            // For now, use a mock embedding (random/zero) for query too
            // In reality this should match the embedding model used for indexing
            let query_vec = vec![0.0; 384];

            let results = store.search(query_vec).await?;
            println!("Found {} results:", results.len());
            for (i, res) in results.iter().enumerate() {
                println!("[{}] {} (Score: {:.4})", i + 1, res.file_path, res.score);
                println!("    Sample: {}\n", res.text.lines().next().unwrap_or(""));
            }
        }
    }

    Ok(())
}
