//! Functions for processing individual contact records.

use crate::config::CONFIG;
use crate::domain::{get_domain_from_url, normalize_url};
use crate::models::{Contact, EmailResult, ProcessingResult, ValidatedContact};
use crate::sleuth::EmailSleuth;
use std::sync::Arc; // For Arc<EmailSleuth>

/// Processes a single contact record to find and verify an email address.
///
/// # Arguments
/// * `sleuth` - An Arc-wrapped `EmailSleuth` instance containing shared clients.
/// * `record` - The input `Contact` record.
///
/// # Returns
/// * `ProcessingResult` containing the original input and the discovery results or errors.
pub(crate) async fn process_record(sleuth: Arc<EmailSleuth>, record: Contact) -> ProcessingResult {
    let record_id = record
        .full_name
        .as_deref()
        .or(record.domain.as_deref())
        .unwrap_or("Unknown Record");

    let task_id = format!(
        "Record: {} | Thread: {:?}",
        record_id,
        std::thread::current().id()
    );
    tracing::info!(target: "process_record_task", "[{}] Starting processing.", task_id);

    let mut first_name = record
        .first_name
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let mut last_name = record.last_name.as_deref().unwrap_or("").trim().to_string();
    let original_full_name = record.full_name.as_deref().unwrap_or("").trim().to_string();
    let domain_input_str = record
        .domain
        .as_deref()
        .or(record.company_domain.as_deref())
        .unwrap_or("")
        .trim()
        .to_string();

    tracing::debug!(target: "process_record_task",
        "[{}] Input: FN='{}', LN='{}', Full(Original)='{}', DomainInput='{}'",
        task_id, first_name, last_name, original_full_name, domain_input_str
    );

    if (first_name.is_empty() || last_name.is_empty()) && !original_full_name.is_empty() {
        let name_parts: Vec<&str> = original_full_name.split_whitespace().collect();
        if name_parts.len() >= 2 {
            if first_name.is_empty() {
                first_name = name_parts[0].to_string();
            }
            if last_name.is_empty() {
                last_name = name_parts.last().unwrap_or(&"").to_string();
            }
            tracing::debug!(target: "process_record_task", "[{}] Derived names: First='{}', Last='{}'", task_id, first_name, last_name);
        } else if name_parts.len() == 1 {
            if first_name.is_empty() && last_name.is_empty() {
                first_name = name_parts[0].to_string();
                last_name = name_parts[0].to_string();
                tracing::debug!(target: "process_record_task", "[{}] Derived single name part for both: '{}'", task_id, name_parts[0]);
            } else if first_name.is_empty() {
                first_name = name_parts[0].to_string();
                tracing::debug!(target: "process_record_task", "[{}] Derived single first name part from full: '{}'", task_id, name_parts[0]);
            } else {
                last_name = name_parts[0].to_string();
                tracing::debug!(target: "process_record_task", "[{}] Derived single last name part from full: '{}'", task_id, name_parts[0]);
            }
        }
    }

    let mut missing_parts = Vec::new();
    if first_name.is_empty() {
        missing_parts.push("first name");
    }
    if last_name.is_empty() {
        missing_parts.push("last name");
    }
    if domain_input_str.is_empty() {
        missing_parts.push("domain");
    }

    if !missing_parts.is_empty() {
        let reason = format!("Missing {}", missing_parts.join(", "));
        tracing::warn!(target: "process_record_task", "[{}] Skipping record. Reason: {}", task_id, reason);
        return ProcessingResult {
            contact_input: record,
            email_discovery_results: None,
            email: None,
            email_confidence: None,
            email_verification_method: None,
            email_alternatives: vec![],
            email_finding_skipped: true,
            email_finding_reason: Some(reason),
            email_verification_failed: false,
            email_finding_error: None,
        };
    }

    let domain = match get_domain_from_url(&domain_input_str) {
        Ok(d) => d,
        Err(e) => {
            let reason = format!(
                "Cannot extract domain from input '{}': {}",
                domain_input_str, e
            );
            tracing::error!(target: "process_record_task", "[{}] Skipping record. Reason: {}", task_id, reason);
            return ProcessingResult {
                contact_input: record,
                email_discovery_results: None,
                email: None,
                email_confidence: None,
                email_verification_method: None,
                email_alternatives: vec![],
                email_finding_skipped: true,
                email_finding_reason: Some(reason),
                email_verification_failed: false,
                email_finding_error: None,
            };
        }
    };

    let website_url = match normalize_url(&domain_input_str) {
        Ok(url) => url,
        Err(e) => {
            let reason = format!(
                "Invalid input for URL normalization '{}': {}",
                domain_input_str, e
            );
            tracing::error!(target: "process_record_task", "[{}] Skipping record. Reason: {}", task_id, reason);
            return ProcessingResult {
                contact_input: record,
                email_discovery_results: None,
                email: None,
                email_confidence: None,
                email_verification_method: None,
                email_alternatives: vec![],
                email_finding_skipped: true,
                email_finding_reason: Some(reason),
                email_verification_failed: false,
                email_finding_error: None,
            };
        }
    };

    let final_full_name = if !original_full_name.is_empty() {
        original_full_name // Use the trimmed original
    } else {
        format!("{} {}", first_name, last_name).trim().to_string()
    };
    tracing::debug!(target: "process_record_task", "[{}] Using final full name: '{}'", task_id, final_full_name);

    let validated_contact = ValidatedContact {
        first_name,
        last_name,
        full_name: final_full_name,
        website_url,
        domain,
        original_contact: record.clone(),
    };

    let find_result: std::result::Result<EmailResult, crate::error::AppError> =
        sleuth.find_email(&validated_contact).await;

    match find_result {
        Ok(results) => {
            let mut final_record = ProcessingResult {
                contact_input: record,
                email_discovery_results: Some(results.clone()),
                email: results.most_likely_email.clone(),
                email_confidence: results
                    .most_likely_email
                    .as_ref()
                    .map(|_| results.confidence_score),
                email_verification_method: Some(results.methods_used.join(", ")),
                email_alternatives: results
                    .found_emails
                    .iter()
                    .filter(|e| Some(&e.email) != results.most_likely_email.as_ref())
                    .take(CONFIG.max_alternatives)
                    .map(|e| e.email.clone())
                    .collect(),
                email_finding_skipped: false,
                email_finding_reason: None,
                email_verification_failed: results.most_likely_email.is_none()
                    && !results.found_emails.is_empty(),
                email_finding_error: None,
            };

            if final_record.email.is_some() {
                tracing::info!(target: "process_record_task",
                    "[{}] ✓ Found likely email: {} (Confidence: {}/10)",
                    task_id, final_record.email.as_ref().unwrap(), final_record.email_confidence.unwrap()
                );
                final_record.email_verification_failed = false;
            } else {
                tracing::info!(target: "process_record_task", "[{}] ✗ No high-confidence email found.", task_id);
                if !results.found_emails.is_empty() {
                    final_record.email_verification_failed = true;
                }
            }
            tracing::info!(target: "process_record_task", "[{}] Finished processing.", task_id);
            final_record
        }
        Err(e) => {
            tracing::error!(target: "process_record_task",
                "[{}] !!! Unexpected error during find_email execution: {}", task_id, e
            );
            ProcessingResult {
                contact_input: record,
                email_discovery_results: None,
                email: None,
                email_confidence: None,
                email_verification_method: None,
                email_alternatives: vec![],
                email_finding_skipped: false,
                email_finding_reason: None,
                email_verification_failed: false,
                email_finding_error: Some(format!("Core processing error: {}", e)),
            }
        }
    }
}
