use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

mod api;
mod config;
mod dns;
mod domain;
mod error;
mod models;
mod patterns;
mod processor;
mod scraper;
mod sleuth;
mod smtp;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process a JSON file containing contact records
    Process {
        /// Path to the input JSON file
        #[arg(short, long)]
        input: PathBuf,

        /// Path to the output JSON file
        #[arg(short, long)]
        output: PathBuf,

        /// Number of concurrent workers
        #[arg(short, long, default_value_t = 5)]
        workers: usize,
    },
    /// Start the API server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration
    config::load_config()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Process {
            input,
            output,
            workers,
        } => {
            info!("Processing contacts from {} to {}", input.display(), output.display());
            process_file(input, output, workers).await?;
        }
        Commands::Serve { port } => {
            info!("Starting API server on port {}", port);
            api::start_api_server(port).await?;
        }
    }

    Ok(())
}

async fn process_file(input: PathBuf, output: PathBuf, workers: usize) -> Result<()> {
    // Read the input file
    let input_data = std::fs::read_to_string(&input)?;
    let contacts: Vec<models::Contact> = serde_json::from_str(&input_data)?;

    info!("Loaded {} contacts from {}", contacts.len(), input.display());

    // Create the EmailSleuth instance
    let sleuth = std::sync::Arc::new(sleuth::EmailSleuth::new().await?);

    // Process the contacts
    let mut results = Vec::with_capacity(contacts.len());
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(workers));

    let progress_bar = indicatif::ProgressBar::new(contacts.len() as u64);
    progress_bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut tasks = Vec::new();

    for contact in contacts {
        let sleuth_clone = sleuth.clone();
        let semaphore_clone = semaphore.clone();
        let progress_bar_clone = progress_bar.clone();

        let task = tokio::spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();
            let result = processor::process_record(sleuth_clone, contact).await;
            progress_bar_clone.inc(1);
            result
        });

        tasks.push(task);
    }

    for task in tasks {
        let result = task.await?;
        results.push(result);
    }

    progress_bar.finish_with_message("Processing complete");

    // Write the results to the output file
    let output_data = serde_json::to_string_pretty(&results)?;
    std::fs::write(&output, output_data)?;

    info!("Wrote {} results to {}", results.len(), output.display());

    Ok(())
}
