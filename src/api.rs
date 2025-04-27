//! API server for email-sleuth.

use crate::models::{Contact, ProcessingResult};
use crate::processor::process_record;
use crate::sleuth::EmailSleuth;
use std::sync::Arc;
use tokio::sync::Semaphore;
use warp::{http::StatusCode, Filter, Rejection, Reply};
use serde::{Deserialize, Serialize};

/// API response structure
#[derive(Serialize, Deserialize)]
struct ApiResponse {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ProcessingResult>,
}

/// Batch API request structure
#[derive(Deserialize)]
struct BatchRequest {
    contacts: Vec<Contact>,
}

/// Batch API response structure
#[derive(Serialize)]
struct BatchResponse {
    success: bool,
    message: String,
    results: Vec<ProcessingResult>,
}

/// Start the API server
pub async fn start_api_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let sleuth = Arc::new(EmailSleuth::new().await?);
    let sleuth_filter = warp::any().map(move || sleuth.clone());
    
    // Limit concurrent requests
    let semaphore = Arc::new(Semaphore::new(10));
    let semaphore_filter = warp::any().map(move || semaphore.clone());
    
    // Health check endpoint
    let health = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::json(&ApiResponse {
            success: true,
            message: "Email Sleuth API is running".to_string(),
            result: None,
        }));
    
    // Single contact verification endpoint
    let verify = warp::path("verify")
        .and(warp::post())
        .and(warp::body::json())
        .and(sleuth_filter.clone())
        .and(semaphore_filter.clone())
        .and_then(handle_verify);
    
    // Batch verification endpoint
    let batch = warp::path("batch")
        .and(warp::post())
        .and(warp::body::json())
        .and(sleuth_filter.clone())
        .and(semaphore_filter.clone())
        .and_then(handle_batch);
    
    // Serve static files for the UI
    let ui = warp::path("ui")
        .and(warp::fs::dir("ui"));
    
    // Redirect root to UI
    let root = warp::path::end()
        .and(warp::get())
        .map(|| warp::redirect::temporary(warp::http::Uri::from_static("/ui")));
    
    // Combine all routes
    let routes = health
        .or(verify)
        .or(batch)
        .or(ui)
        .or(root)
        .with(warp::cors().allow_any_origin());
    
    tracing::info!("Starting API server on port {}", port);
    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    
    Ok(())
}

/// Handle a single contact verification request
async fn handle_verify(
    contact: Contact,
    sleuth: Arc<EmailSleuth>,
    semaphore: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
    let _permit = semaphore.acquire().await.map_err(|_| warp::reject::custom(ApiError))?;
    
    tracing::info!("Processing single contact verification request");
    let result = process_record(sleuth, contact).await;
    
    Ok(warp::reply::json(&ApiResponse {
        success: true,
        message: "Contact processed successfully".to_string(),
        result: Some(result),
    }))
}

/// Handle a batch verification request
async fn handle_batch(
    batch: BatchRequest,
    sleuth: Arc<EmailSleuth>,
    semaphore: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
    tracing::info!("Processing batch of {} contacts", batch.contacts.len());
    
    let mut results = Vec::with_capacity(batch.contacts.len());
    
    for contact in batch.contacts {
        let _permit = semaphore.acquire().await.map_err(|_| warp::reject::custom(ApiError))?;
        let result = process_record(sleuth.clone(), contact).await;
        results.push(result);
    }
    
    Ok(warp::reply::json(&BatchResponse {
        success: true,
        message: format!("Processed {} contacts", results.len()),
        results,
    }))
}

/// Custom error type for API rejections
#[derive(Debug)]
struct ApiError;

impl warp::reject::Reject for ApiError {}

/// Handle API rejections
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
    if err.is_not_found() {
        Ok(warp::reply::with_status(
            warp::reply::json(&ApiResponse {
                success: false,
                message: "Not Found".to_string(),
                result: None,
            }),
            StatusCode::NOT_FOUND,
        ))
    } else if let Some(_) = err.find::<ApiError>() {
        Ok(warp::reply::with_status(
            warp::reply::json(&ApiResponse {
                success: false,
                message: "Server error".to_string(),
                result: None,
            }),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    } else {
        Ok(warp::reply::with_status(
            warp::reply::json(&ApiResponse {
                success: false,
                message: "Bad request".to_string(),
                result: None,
            }),
            StatusCode::BAD_REQUEST,
        ))
    }
}
