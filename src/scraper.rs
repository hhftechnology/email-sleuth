//! Functions for scraping website content to find email addresses.

use crate::config::{CONFIG, get_random_sleep_duration};
use crate::error::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::{HashSet, VecDeque};
use std::time::Instant;
use url::Url;

/// Scrapes a website (starting URL + common pages) to find email addresses.
///
/// # Arguments
/// * `http_client` - A shared `reqwest::Client` instance.
/// * `base_url` - The starting URL of the website to scrape.
/// * `target_domain` - The primary domain we are interested in emails for (lowercase).
///
/// # Returns
/// * `Result<Vec<String>>` containing a list of unique, potentially valid email addresses found.
pub(crate) async fn scrape_website_for_emails(
    http_client: &Client,
    base_url: &Url,
) -> Result<Vec<String>> {
    let start_time = Instant::now();
    tracing::info!(target: "scrape_task", "Starting scrape for: {}", base_url);

    let mut found_emails: HashSet<String> = HashSet::new();
    let mut processed_urls: HashSet<String> = HashSet::new();
    let mut urls_to_visit: VecDeque<Url> = VecDeque::new();
    let mut successful_pages = 0;
    let mut failed_pages = 0;

    urls_to_visit.push_back(base_url.clone());
    for page_path in &CONFIG.common_pages_to_scrape {
        match base_url.join(page_path) {
            Ok(full_url) => {
                if full_url.domain() == base_url.domain() {
                    urls_to_visit.push_back(full_url);
                } else {
                    tracing::debug!("Skipping generated URL (different domain): {}", full_url);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to join base URL {} with page {}: {}",
                    base_url,
                    page_path,
                    e
                );
            }
        }
    }

    let initial_urls: HashSet<Url> = urls_to_visit.into_iter().collect();
    let initial_urls_count = initial_urls.len();
    urls_to_visit = initial_urls.into_iter().collect();

    tracing::debug!(target: "scrape_task", "Planning to scrape {} potential URLs.", urls_to_visit.len());

    let mut any_page_successful = false;

    use once_cell::sync::Lazy;
    static EMAIL_LINK_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("a[href^='mailto:']").unwrap());

    while let Some(page_url) = urls_to_visit.pop_front() {
        let url_string = page_url.to_string();
        if processed_urls.contains(&url_string) {
            continue;
        }
        processed_urls.insert(url_string.clone());

        tracing::debug!(target: "scrape_task", "Attempting to GET: {}", page_url);

        let response_result = http_client
            .get(page_url.clone())
            .timeout(CONFIG.request_timeout)
            .send()
            .await;

        match response_result {
            Ok(response) => {
                let status = response.status();
                tracing::debug!(target: "scrape_task", "GET {} status: {}", page_url, status);

                if status.is_success() {
                    any_page_successful = true;
                    let content_type = response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|val| val.to_str().ok())
                        .unwrap_or("")
                        .to_lowercase();

                    if !content_type.contains("html") {
                        tracing::debug!(
                            target: "scrape_task",
                            "Skipping non-HTML content at {} ({})", page_url, content_type
                        );
                        continue;
                    }

                    match response.text().await {
                        Ok(html_content) => {
                            successful_pages += 1;

                            {
                                let document = Html::parse_document(&html_content);

                                for element in document.select(&EMAIL_LINK_SELECTOR) {
                                    if let Some(href) = element.value().attr("href") {
                                        if let Some(email_part) = href.strip_prefix("mailto:") {
                                            let email =
                                                email_part.split('?').next().unwrap_or("").trim();
                                            if !email.is_empty()
                                                && CONFIG.email_regex.is_match(email)
                                            {
                                                tracing::debug!(target: "scrape_task", "Found via mailto link ({}): {}", page_url, email);
                                                found_emails.insert(email.to_lowercase());
                                            } else if !email.is_empty() {
                                                tracing::warn!(target: "scrape_task", "Mailto content failed regex check: {}", email);
                                            }
                                        }
                                    }
                                }

                                let mut text_content = String::new();

                                let body_selector = Selector::parse("body").unwrap();
                                if let Some(body_node) = document.select(&body_selector).next() {
                                    for text_fragment in body_node.text() {
                                        text_content.push_str(text_fragment.trim());
                                        text_content.push(' ');
                                    }
                                } else {
                                    for text_fragment in document.root_element().text() {
                                        text_content.push_str(text_fragment.trim());
                                        text_content.push(' ');
                                    }
                                }

                                for email_match in CONFIG.email_regex.find_iter(&text_content) {
                                    let email = email_match.as_str();
                                    tracing::debug!(target: "scrape_task", "Found via regex in text ({}): {}", page_url, email);
                                    found_emails.insert(email.to_lowercase());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(target: "scrape_task", "Failed to read text content from {}: {}", page_url, e);
                            failed_pages += 1;
                        }
                    }
                } else if status == reqwest::StatusCode::NOT_FOUND {
                    failed_pages += 1;
                    tracing::debug!(target: "scrape_task", "Page not found (404): {}", page_url);
                } else if status.is_client_error() || status.is_server_error() {
                    failed_pages += 1;
                    tracing::warn!(target: "scrape_task", "HTTP error scraping {}: {}", page_url, status);
                }
            }
            Err(e) => {
                failed_pages += 1;
                if e.is_timeout() {
                    tracing::warn!(target: "scrape_task", "Timeout scraping {}: {}", page_url, e);
                } else if e.is_connect() || e.is_request() {
                    tracing::warn!(target: "scrape_task", "Request/Connection error scraping {}: {}", page_url, e);
                } else {
                    tracing::warn!(target: "scrape_task", "Unexpected error scraping {}: {}", page_url, e);
                }
            }
        }
    }

    if !any_page_successful && initial_urls_count > 0 {
        tracing::warn!(target: "scrape_task", "Could not successfully scrape any pages for {}", base_url);
    }

    let filtered_emails: Vec<String> = found_emails
        .into_iter()
        .filter(|email| {
            if let Some((_local, domain_part)) = email.rsplit_once('@') {
                // Basic validity check on domain part
                domain_part.contains('.') && domain_part.len() > 3 // e.g., a.co
            } else {
                false // Should not happen if regex matched, but be safe
            }
        })
        .collect();

    let elapsed = start_time.elapsed();
    tracing::info!(
        target: "scrape_task",
        "Scrape for {} finished in {:.2?}. Attempted {} URLs ({} successful, {} failed). Found {} potentially valid emails.",
        base_url,
        elapsed,
        successful_pages + failed_pages,
        successful_pages,
        failed_pages,
        filtered_emails.len()
    );

    Ok(filtered_emails)
}
