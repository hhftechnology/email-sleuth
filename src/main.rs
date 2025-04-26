//! # Email Sleuth RS
//!
//! A Rust application to discover and verify professional email addresses
//! based on contact names and company websites. Inspired by a similar Python tool.
//! This serves as the main entry point for the application.

#![warn(missing_docs, unreachable_pub, rust_2018_idioms)]

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

use crate::config::CONFIG;
use crate::models::{Contact, ProcessingResult};
use crate::processor::process_record;
use crate::sleuth::EmailSleuth;

use anyhow::{Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use smtp::test_smtp_connectivity;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;

/// Main entry point for the Email Sleuth application.
///
/// Initializes logging, loads configuration, reads input data,
/// processes contacts concurrently, and writes results.
#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_thread_names(true)
        .with_target(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default tracing subscriber failed");

    tracing::info!(
        "Logging initialized. Starting Email Sleuth RS v{}...",
        env!("CARGO_PKG_VERSION")
    );
    tracing::debug!("Debug logging is enabled.");
    tracing::debug!("Using configuration: {:?}", *CONFIG);

    if let Err(e) = test_smtp_connectivity().await {
        tracing::error!("SMTP connectivity test failed: {}", e);
        tracing::error!(
            "This application requires outbound SMTP (port 25) connectivity to function properly"
        );
        tracing::error!("Common solutions:");
        tracing::error!("1. Use a VPN to bypass port 25 blocking");
        tracing::error!(
            "2. Run this application on a cloud server (most cloud providers allow outbound port 25)"
        );

        return Err(anyhow::anyhow!("SMTP connectivity check failed: {}", e));
    }

    let start_time = std::time::Instant::now();

    if CONFIG.cli_mode {
        process_cli_mode().await?;
    } else {
        process_file_mode().await?;
    }

    tracing::info!(
        "Script finished successfully. Duration: {:.2?}",
        start_time.elapsed()
    );
    Ok(())
}

/// Process a single contact provided via command line arguments
async fn process_cli_mode() -> Result<()> {
    tracing::info!("Running in CLI mode");
    let name = CONFIG.cli_name.as_ref().unwrap();
    let domain_input = CONFIG.cli_domain.as_ref().unwrap();

    let name_parts: Vec<&str> = name.split_whitespace().collect();
    let first_name = if !name_parts.is_empty() {
        Some(name_parts[0].to_string())
    } else {
        None
    };
    let last_name = if name_parts.len() > 1 {
        Some(name_parts.last().unwrap().to_string())
    } else {
        first_name.clone()
    };

    let contact = Contact {
        first_name,
        last_name,
        full_name: Some(name.clone()),
        domain: Some(domain_input.clone()),
        company_domain: None,
        other_fields: std::collections::HashMap::new(),
    };

    tracing::info!("Finding email for name: {}, domain: {}", name, domain_input);

    let sleuth = Arc::new(
        EmailSleuth::new()
            .await
            .context("Failed to initialize EmailSleuth")?,
    );

    let result = process_record(sleuth, contact).await;

    if CONFIG.output_to_stdout {
        print_cli_results(&result);
    } else {
        tracing::info!("Saving result to '{}'...", CONFIG.output_file);
        save_results(&[result], &CONFIG.output_file)?;
        tracing::info!("Result saved successfully to '{}'.", CONFIG.output_file);
    }

    Ok(())
}

/// Print CLI results to stdout in a human-readable format
fn print_cli_results(result: &ProcessingResult) {
    println!("\n===== Email Sleuth Results =====");
    println!(
        "Name: {}",
        result.contact_input.full_name.as_deref().unwrap_or("N/A")
    );
    println!(
        "Domain: {}",
        result.contact_input.domain.as_deref().unwrap_or("N/A")
    );

    if result.email_finding_skipped {
        println!("\nStatus: SKIPPED");
        println!(
            "Reason: {}",
            result.email_finding_reason.as_deref().unwrap_or("Unknown")
        );
    } else if let Some(error) = &result.email_finding_error {
        println!("\nStatus: ERROR");
        println!("Error: {}", error);
    } else if let Some(email) = &result.email {
        println!("\nStatus: SUCCESS");
        println!("Email: {}", email);
        println!("Confidence: {}/10", result.email_confidence.unwrap_or(0));
        println!(
            "Method: {}",
            result
                .email_verification_method
                .as_deref()
                .unwrap_or("Unknown")
        );

        if !result.email_alternatives.is_empty() {
            println!("\nAlternative emails:");
            for alt in &result.email_alternatives {
                println!("- {}", alt);
            }
        }
    } else {
        println!("\nStatus: NO EMAIL FOUND");
        if result.email_verification_failed {
            println!("Verification failed for potential candidates");
        }
    }

    if let Some(email_results) = &result.email_discovery_results {
        if !email_results.verification_log.is_empty() {
            println!("\nVerification details:");
            for (email, message) in &email_results.verification_log {
                println!("- {}: {}", email, message);
            }
        }
    }

    println!("==============================\n");
}

/// Process contacts from a file (existing functionality)
async fn process_file_mode() -> Result<()> {
    let start_time = std::time::Instant::now();
    let input_path = Path::new(&CONFIG.input_file);
    if !input_path.exists() {
        tracing::error!("Input file '{}' not found", CONFIG.input_file);
        tracing::error!(
            "Please check that the file exists and that you have permission to read it"
        );
        tracing::error!("Use --input to specify a different input file");
        return Err(anyhow::anyhow!(
            "Input file not found: {}",
            CONFIG.input_file
        ));
    }

    if !input_path.is_file() {
        tracing::error!("'{}' is not a file", CONFIG.input_file);
        return Err(anyhow::anyhow!(
            "Input path is not a file: {}",
            CONFIG.input_file
        ));
    }

    let output_path = Path::new(&CONFIG.output_file);
    if let Some(parent_dir) = output_path.parent() {
        if !parent_dir.exists() && !parent_dir.as_os_str().is_empty() {
            tracing::error!("Output directory '{}' does not exist", parent_dir.display());
            return Err(anyhow::anyhow!(
                "Output directory does not exist: {}",
                parent_dir.display()
            ));
        }

        if !parent_dir.as_os_str().is_empty() {
            match std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&CONFIG.output_file)
            {
                Ok(_) => {
                    if output_path.exists() {
                        let _ = std::fs::remove_file(&CONFIG.output_file);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Cannot write to output file '{}': {}",
                        CONFIG.output_file,
                        e
                    );
                    return Err(anyhow::anyhow!("Cannot write to output file: {}", e));
                }
            }
        }
    }

    tracing::info!("Loading input data from '{}'...", CONFIG.input_file);
    let records: Vec<Contact> = match load_contacts(&CONFIG.input_file) {
        Ok(records) => records,
        Err(e) => {
            tracing::error!(
                "Failed to load contacts from '{}': {}",
                CONFIG.input_file,
                e
            );
            tracing::error!("Please ensure the file contains valid JSON in the expected format");
            tracing::error!(
                "Example format: [{{\"first_name\":\"John\",\"last_name\":\"Doe\",\"company_domain\":\"example.com\"}}]"
            );
            return Err(anyhow::anyhow!("Failed to parse input file: {}", e));
        }
    };

    let total_records = records.len();
    if total_records == 0 {
        tracing::warn!(
            "Input file '{}' contains zero records. Exiting.",
            CONFIG.input_file
        );
        return Ok(());
    }

    let mut invalid_records = 0;
    for (i, record) in records.iter().enumerate() {
        let has_name = record.full_name.is_some()
            || (record.first_name.is_some() && record.last_name.is_some());

        let has_domain = record.domain.is_some() || record.company_domain.is_some();

        if !has_name || !has_domain {
            tracing::warn!(
                "Record #{} is missing required fields. Each record needs either 'full_name' or both 'first_name' and 'last_name', plus 'domain'",
                i + 1
            );
            invalid_records += 1;
        }
    }

    if invalid_records > 0 {
        tracing::warn!(
            "{} out of {} records are missing required fields. These records will be skipped during processing.",
            invalid_records,
            total_records
        );

        if invalid_records == total_records {
            tracing::error!("All records are invalid. Please check your input file format.");
            tracing::error!(
                "Expected format: [{{\"first_name\":\"John\",\"last_name\":\"Doe\",\"domain\":\"example.com\"}}]"
            );
            return Err(anyhow::anyhow!("All records in input file are invalid"));
        }
    }

    tracing::info!(
        "Loaded {} valid records for processing.",
        total_records - invalid_records
    );

    tracing::debug!("Initializing EmailSleuth instance...");
    let sleuth = Arc::new(
        EmailSleuth::new()
            .await
            .context("Failed to initialize EmailSleuth")?,
    );
    tracing::debug!("EmailSleuth initialized.");

    tracing::info!(
        "Starting email discovery for {} records using up to {} concurrent tasks...",
        total_records,
        CONFIG.max_concurrency
    );

    let pb = ProgressBar::new(total_records as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | ETA: {eta_precise}")?
        .progress_chars("#>-"));

    let mut tasks = FuturesUnordered::new();
    let mut processed_records = Vec::with_capacity(total_records);

    for record in records {
        if tasks.len() >= CONFIG.max_concurrency {
            if let Some(result) = tasks.next().await {
                match result {
                    Ok(processed) => {
                        processed_records.push(processed);
                        pb.inc(1);
                    }
                    Err(e) => {
                        tracing::error!("A processing task panicked: {}", e);
                        pb.inc(1);
                    }
                }
            }
        }

        let sleuth_clone = Arc::clone(&sleuth);
        tasks.push(tokio::spawn(async move {
            process_record(sleuth_clone, record).await
        }));
    }

    // Process remaining tasks
    while let Some(result) = tasks.next().await {
        match result {
            Ok(processed) => {
                processed_records.push(processed);
                pb.inc(1);
            }
            Err(e) => {
                tracing::error!("A processing task panicked: {}", e);
                pb.inc(1);
            }
        }
    }

    pb.finish_with_message("Processing complete");

    let output_file = &CONFIG.output_file;
    tracing::info!(
        "Processing finished. Saving {} results to '{}'...",
        processed_records.len(),
        output_file
    );

    processed_records.sort_by(|a, b| {
        let name_a = a
            .contact_input
            .full_name
            .as_deref()
            .or(a.contact_input.first_name.as_deref())
            .unwrap_or("");
        let name_b = b
            .contact_input
            .full_name
            .as_deref()
            .or(b.contact_input.first_name.as_deref())
            .unwrap_or("");
        name_a.cmp(name_b)
    });

    match save_results(&processed_records, output_file) {
        Ok(_) => tracing::info!("Results saved successfully to '{}'.", output_file),
        Err(e) => {
            tracing::error!("Failed to save results to '{}': {}", output_file, e);
            return Err(anyhow::anyhow!("Failed to save results: {}", e));
        }
    }

    let duration = start_time.elapsed();
    log_summary(&processed_records, total_records, duration);

    Ok(())
}

/// Loads contact records from the specified JSON file with improved error handling.
fn load_contacts(file_path: &str) -> Result<Vec<Contact>> {
    let file = File::open(file_path)
        .with_context(|| format!("Failed to open input file '{}' for reading", file_path))?;

    let reader = BufReader::new(file);

    let records: serde_json::Result<Vec<Contact>> = serde_json::from_reader(reader);

    match records {
        Ok(data) => Ok(data),
        Err(e) => {
            if e.is_syntax() {
                Err(anyhow::anyhow!(
                    "JSON syntax error in '{}': {}",
                    file_path,
                    e
                ))
            } else if e.is_data() {
                Err(anyhow::anyhow!(
                    "JSON data structure doesn't match expected format in '{}': {}",
                    file_path,
                    e
                ))
            } else if e.is_eof() {
                Err(anyhow::anyhow!(
                    "Unexpected end of JSON data in '{}': {}",
                    file_path,
                    e
                ))
            } else {
                Err(anyhow::anyhow!(
                    "Error parsing JSON from '{}': {}",
                    file_path,
                    e
                ))
            }
        }
    }
}

/// Saves the processed results to the specified JSON file with improved error handling.
fn save_results(results: &[ProcessingResult], file_path: &str) -> Result<()> {
    let file = File::create(file_path)
        .with_context(|| format!("Failed to create output file '{}' for writing", file_path))?;

    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, results)
        .with_context(|| format!("Failed to serialize results to JSON for '{}'", file_path))?;

    Ok(())
}

/// Logs a summary of the processing results.
fn log_summary(processed_records: &[ProcessingResult], original_total: usize, duration: Duration) {
    let effective_total = processed_records.len();
    let likely_emails_found = processed_records
        .iter()
        .filter(|r| {
            r.email.is_some() && !r.email_finding_skipped && r.email_finding_error.is_none()
        })
        .count();
    let skipped_count = processed_records
        .iter()
        .filter(|r| r.email_finding_skipped)
        .count();
    let error_count = processed_records
        .iter()
        .filter(|r| r.email_finding_error.is_some())
        .count();
    let failed_or_low_confidence = effective_total
        .saturating_sub(likely_emails_found)
        .saturating_sub(skipped_count)
        .saturating_sub(error_count);

    tracing::info!("-------------------- Summary --------------------");
    tracing::info!("Total Records Input     : {}", original_total);
    tracing::info!(
        "Total Records Processed : {}/{}",
        effective_total,
        original_total
    );
    tracing::info!("Likely Emails Found     : {}", likely_emails_found);
    tracing::info!("Failed / Low Confidence : {}", failed_or_low_confidence);
    tracing::info!("Records Skipped (Input) : {}", skipped_count);
    tracing::info!("Records with Errors     : {}", error_count);
    tracing::info!("Total Time Taken        : {:.2?}", duration);
    tracing::info!("-------------------------------------------------");
}
