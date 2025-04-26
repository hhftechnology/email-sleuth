//! Core logic for finding and verifying email addresses.

use crate::config::{CONFIG, get_random_sleep_duration};
use crate::dns::{create_resolver, resolve_mail_server};
use crate::error::{AppError, Result};
use crate::models::{EmailResult, FoundEmailData, ValidatedContact};
use crate::patterns::generate_email_patterns;
use crate::scraper::scrape_website_for_emails;
use crate::smtp::verify_email_smtp_with_retries;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::Arc; // For sharing clients
use std::time::Duration;
use tokio::time::sleep;
use trust_dns_resolver::TokioAsyncResolver;
#[derive(Debug, Clone)]
pub(crate) struct EmailSleuth {
    http_client: Arc<Client>,
    dns_resolver: Arc<TokioAsyncResolver>,
}

impl EmailSleuth {
    /// Creates a new EmailSleuth instance with shared HTTP and DNS clients.
    pub(crate) async fn new() -> Result<Self> {
        let http_client = Arc::new(
            Client::builder()
                .user_agent(&CONFIG.user_agent)
                .timeout(CONFIG.request_timeout)
                .build()
                .map_err(|e| {
                    AppError::Generic(anyhow::anyhow!("Failed to build HTTP client: {}", e))
                })?,
        );

        let dns_resolver = Arc::new(create_resolver().await?);

        Ok(Self {
            http_client,
            dns_resolver,
        })
    }

    /// Finds and verifies email addresses for a given contact.
    ///
    /// This is the main entry point for the finding logic corresponding to the
    /// Python class's `find_email` method.
    ///
    /// # Arguments
    /// * `contact` - A validated contact with required information.
    ///
    /// # Returns
    /// * `Result<EmailResult>` containing the findings.
    pub(crate) async fn find_email(&self, contact: &ValidatedContact) -> Result<EmailResult> {
        tracing::info!(target: "find_email_task",
            "Finding email for: {} {}, Website: {}",
            contact.first_name,
            contact.last_name,
            contact.website_url
        );

        let mut results = EmailResult::default();
        let domain = &contact.domain;

        tracing::debug!(target: "find_email_task", "Starting pattern generation...");
        let generated_patterns =
            generate_email_patterns(&contact.first_name, &contact.last_name, domain);
        if !generated_patterns.is_empty() {
            results.methods_used.push("pattern_generation".to_string());
            tracing::debug!(target: "find_email_task", "Finished pattern generation ({} patterns).", generated_patterns.len());
        }

        tracing::debug!(target: "find_email_task", "Starting website scraping...");
        let scraped_emails_raw =
            scrape_website_for_emails(&self.http_client, &contact.website_url).await?;
        let scraped_emails: Vec<String> = scraped_emails_raw
            .iter()
            .filter(|email| {
                email.ends_with(&format!("@{}", domain)) || self.is_generic_prefix(email)
            })
            .cloned()
            .collect();

        if !scraped_emails.is_empty() {
            results.methods_used.push("website_scraping".to_string());
            tracing::info!(target: "find_email_task",
                "Found {} relevant emails via scraping.",
                scraped_emails.len()
            );
        }
        tracing::debug!(target: "find_email_task", "Finished website scraping.");

        tracing::debug!(target: "find_email_task", "Combining and ordering candidates...");
        let mut all_candidates = Vec::new();
        let mut seen_candidates = HashSet::new();

        let first_lower = contact.first_name.to_lowercase();
        let last_lower = contact.last_name.to_lowercase();

        let add_candidate = |email: &str, list: &mut Vec<String>, seen: &mut HashSet<String>| {
            if !email.is_empty() && seen.insert(email.to_lowercase()) {
                list.push(email.to_lowercase());
            }
        };

        for p in &generated_patterns {
            if p.contains(&first_lower) || p.contains(&last_lower) {
                add_candidate(p, &mut all_candidates, &mut seen_candidates);
            }
        }
        for s in &scraped_emails {
            if s.contains(&first_lower) || s.contains(&last_lower) {
                add_candidate(s, &mut all_candidates, &mut seen_candidates);
            }
        }
        for s in &scraped_emails {
            if !(s.contains(&first_lower) || s.contains(&last_lower)) {
                add_candidate(s, &mut all_candidates, &mut seen_candidates);
            }
        }
        for p in &generated_patterns {
            if !(p.contains(&first_lower) || p.contains(&last_lower)) {
                add_candidate(p, &mut all_candidates, &mut seen_candidates);
            }
        }

        tracing::info!(target: "find_email_task",
            "Total unique candidates to assess: {}",
            all_candidates.len()
        );
        tracing::debug!(target: "find_email_task", "Candidate list (ordered): {:?}", all_candidates);

        let mut verified_emails_data: Vec<FoundEmailData> = Vec::new();
        let mail_server_result = resolve_mail_server(&self.dns_resolver, domain).await;

        let mail_server = match mail_server_result {
            Ok(ms) => {
                tracing::info!(target: "find_email_task", "Using mail server {} for domain {}", ms.exchange, domain);
                Some(ms.exchange)
            }
            Err(e) => {
                tracing::warn!(target: "find_email_task",
                    "Failed to resolve mail server for {}: {}. SMTP verification will be skipped.",
                    domain, e
                );
                results
                    .verification_log
                    .insert(domain.to_string(), format!("DNS resolution failed: {}", e));
                None
            }
        };

        tracing::debug!(target: "find_email_task", "Starting candidate verification and scoring...");
        for email in all_candidates {
            if !CONFIG.email_regex.is_match(&email) {
                tracing::warn!(target: "find_email_task", "Skipping invalid candidate format: {}", email);
                continue;
            }

            tracing::debug!(target: "find_email_task", "Assessing candidate: {}", email);
            let mut confidence: i16 = 0;
            let verification_status: Option<bool>;
            let mut verification_message: String = "Verification not attempted".to_string();

            let email_parts: Vec<&str> = email.split('@').collect();
            let email_local_part = email_parts.get(0).cloned().unwrap_or("").to_lowercase();
            let email_domain_part = email_parts.get(1).cloned().unwrap_or("").to_lowercase();

            let is_scraped = scraped_emails.iter().any(|s| s == &email);
            let is_pattern = generated_patterns.iter().any(|p| p == &email);
            let is_generic = self.is_generic_prefix(&email);
            let matches_primary_domain = email_domain_part == *domain;

            if !matches_primary_domain && !(is_scraped && is_generic) {
                tracing::debug!(target: "find_email_task",
                   "Skipping candidate {}: Non-primary domain ({}) and not a scraped generic.",
                   email, email_domain_part
                );
                continue;
            }

            let name_in_email =
                email_local_part.contains(&first_lower) || email_local_part.contains(&last_lower);

            if is_pattern && name_in_email {
                confidence += 3;
            }
            if is_scraped && name_in_email {
                confidence += 5;
            }
            if is_scraped && !name_in_email {
                confidence += 2;
            }
            if is_pattern && !name_in_email {
                confidence += 1;
            }
            if matches_primary_domain {
                confidence += 1;
            }

            tracing::debug!(target: "find_email_task",
               "Base confidence for {}: {} (Scraped: {}, Pattern: {}, NameIn: {}, Generic: {}, DomainMatch: {})",
               email, confidence, is_scraped, is_pattern, name_in_email, is_generic, matches_primary_domain
            );

            if is_generic && name_in_email && confidence > 1 {
                let penalty = 5;
                confidence = std::cmp::max(1, confidence - penalty);
                tracing::debug!(target: "find_email_task",
                   "Applied penalty for generic prefix '{}'. New confidence: {}",
                   email_local_part, confidence
                );
            } else if is_generic && !name_in_email && confidence > 2 {
                let penalty = 2;
                confidence = std::cmp::max(1, confidence - penalty);
                tracing::debug!(target: "find_email_task",
                    "Applied smaller penalty for scraped generic prefix '{}'. New confidence: {}",
                    email_local_part, confidence
                );
            }

            let should_verify_smtp = mail_server.is_some()
                && (confidence >= 3 || (is_scraped && name_in_email && confidence > 1));

            tracing::debug!(target: "find_email_task",
               "Should verify {}? {} (Confidence: {}, MailServer: {:?})",
               email, should_verify_smtp, confidence, mail_server.is_some()
            );

            let mut verification_duration_secs: f64 = 0.0;

            if should_verify_smtp {
                let current_mail_server = mail_server.as_ref().unwrap();
                if !results
                    .methods_used
                    .contains(&"smtp_verification".to_string())
                {
                    results.methods_used.push("smtp_verification".to_string());
                }

                let verify_start_time = std::time::Instant::now();
                let (exists, message) =
                    verify_email_smtp_with_retries(&email, &email_domain_part, current_mail_server)
                        .await;
                verification_duration_secs = verify_start_time.elapsed().as_secs_f64();

                verification_status = exists;
                verification_message = message;
                results.verification_log.insert(
                    email.clone(),
                    format!(
                        "{} (Took {:.2}s)",
                        verification_message, verification_duration_secs
                    ),
                );

                match exists {
                    Some(true) => {
                        let boost = 5;
                        confidence += boost;
                        tracing::debug!(target: "find_email_task", "Applied boost ({}) for successful verification. New confidence: {}", boost, confidence);
                    }
                    Some(false) => {
                        confidence = 0;
                        tracing::debug!(target: "find_email_task", "Reset confidence to 0 due to failed verification.");
                    }
                    None => {
                        // Inconclusive
                        let boost = 1;
                        confidence += boost;
                        tracing::debug!(target: "find_email_task", "Applied small boost ({}) for inconclusive verification. New confidence: {}", boost, confidence);
                    }
                }
            } else {
                verification_status = None;
                if mail_server.is_none() {
                    verification_message = "Verification skipped (DNS lookup failed)".to_string();
                } else {
                    verification_message =
                        "Verification skipped (low initial confidence)".to_string();
                }
                results
                    .verification_log
                    .insert(email.clone(), verification_message.clone());
            }

            let final_confidence = std::cmp::max(0, std::cmp::min(10, confidence)) as u8;

            if final_confidence > 0 {
                tracing::debug!(target: "find_email_task",
                   "Storing final data for {}: Confidence={}, Status={:?}",
                   email, final_confidence, verification_status
                );
                verified_emails_data.push(FoundEmailData {
                    email: email.clone(),
                    confidence: final_confidence,
                    source: if is_scraped { "scraped" } else { "pattern" }.to_string(),
                    is_generic,
                    verification_status,
                    verification_message,
                });
            } else {
                tracing::debug!(target: "find_email_task",
                   "Discarding candidate {} due to zero final confidence.", email
                );
            }

            if should_verify_smtp {
                let base_sleep = get_random_sleep_duration();
                let adaptive_delay = Duration::from_secs_f64(verification_duration_secs * 0.1)
                    .clamp(Duration::ZERO, Duration::from_secs(1));
                let total_sleep = base_sleep + adaptive_delay;
                tracing::debug!(target: "find_email_task",
                    "Sleeping {:?} after verification attempt for {}", total_sleep, email
                );
                sleep(total_sleep).await;
            }
        }

        tracing::debug!(target: "find_email_task", "Sorting verified email data...");

        verified_emails_data.sort_by(|a, b| {
            b.confidence
                .cmp(&a.confidence)
                .then_with(|| a.is_generic.cmp(&b.is_generic))
                .then_with(|| b.source.cmp(&a.source))
        });

        results.found_emails = verified_emails_data;

        tracing::debug!(target: "find_email_task", "Sorted results: {:?}", results.found_emails);

        results.most_likely_email = None;
        results.confidence_score = 0;

        let best_non_generic = results
            .found_emails
            .iter()
            .find(|e| !e.is_generic && e.confidence >= CONFIG.confidence_threshold);

        if let Some(email_data) = best_non_generic {
            results.most_likely_email = Some(email_data.email.clone());
            results.confidence_score = email_data.confidence;
            tracing::info!(target: "find_email_task",
               "Selected best non-generic: {} (Conf: {})",
               email_data.email, email_data.confidence
            );
        } else if let Some(top_candidate) = results.found_emails.first() {
            if top_candidate.confidence >= CONFIG.confidence_threshold {
                if !top_candidate.is_generic
                    || top_candidate.confidence >= CONFIG.generic_confidence_threshold
                {
                    results.most_likely_email = Some(top_candidate.email.clone());
                    results.confidence_score = top_candidate.confidence;
                    if top_candidate.is_generic {
                        tracing::warn!(target: "find_email_task",
                            "Selected top candidate (generic allowed/high conf): {} (Conf: {})",
                            top_candidate.email, top_candidate.confidence
                        );
                    } else {
                        tracing::info!(target: "find_email_task",
                            "Selected top candidate: {} (Conf: {})",
                            top_candidate.email, top_candidate.confidence
                        );
                    }
                } else {
                    tracing::info!(target: "find_email_task",
                       "Top candidate '{}' is generic with moderate confidence ({}). Not selected.",
                       top_candidate.email, top_candidate.confidence
                    );
                }
            } else {
                tracing::info!(target: "find_email_task",
                   "Top candidate '{}' has low confidence ({}). Not selected.",
                   top_candidate.email, top_candidate.confidence
                );
            }
        } else {
            tracing::info!(target: "find_email_task", "No candidates found with confidence > 0.");
        }

        tracing::info!(target: "find_email_task",
            "Finished finding email for: {} {}. Result: {:?}",
            contact.first_name, contact.last_name, results.most_likely_email
        );

        Ok(results)
    }

    fn is_generic_prefix(&self, email: &str) -> bool {
        if let Some(local_part) = email.split('@').next() {
            CONFIG
                .generic_email_prefixes
                .contains(local_part.to_lowercase().as_str())
        } else {
            false
        }
    }
}
