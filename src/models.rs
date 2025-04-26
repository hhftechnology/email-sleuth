//! Defines the core data structures used in the email-sleuth application.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url; // Import Url type

/// Represents the input contact record read from the JSON file.
/// Allows for flexibility if some fields are missing.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Contact {
    /// The contact's first name.
    pub first_name: Option<String>,
    /// The contact's last name.
    pub last_name: Option<String>,
    /// The contact's full name (optional input).
    pub full_name: Option<String>,
    /// The company domain (e.g., "example.com") or a full URL ("https://example.com").
    pub domain: Option<String>,
    /// Alias for domain field to support legacy format
    #[serde(alias = "company_domain")]
    #[serde(skip_serializing)]
    pub company_domain: Option<String>,
    // Allow capturing other fields from the input JSON
    #[serde(flatten)]
    pub other_fields: HashMap<String, serde_json::Value>,
}

/// Represents a single email address found and its associated metadata.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FoundEmailData {
    /// The discovered email address.
    pub email: String,
    /// A score indicating the likelihood of this email being correct (0-10).
    pub confidence: u8,
    /// The method used to discover this email ("pattern" or "scraped").
    pub source: String, // Could be an enum: Source { Pattern, Scraped }
    /// Indicates if the email address uses a common generic prefix (e.g., info@, contact@).
    pub is_generic: bool,
    /// The result of the SMTP verification attempt (True=Verified, False=Rejected, None=Inconclusive/Untested).
    pub verification_status: Option<bool>,
    /// A message accompanying the verification status (e.g., error details, OK message).
    pub verification_message: String,
}

/// Contains the results of the email finding process for a single contact.
/// This structure will be added to the original Contact data before saving.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub(crate) struct EmailResult {
    /// A list of all potentially valid emails found, ordered by likelihood.
    pub found_emails: Vec<FoundEmailData>,
    /// The single email address deemed most likely to be correct.
    pub most_likely_email: Option<String>,
    /// The confidence score associated with the most_likely_email.
    pub confidence_score: u8,
    /// List of methods used during the discovery process (e.g., "pattern_generation", "website_scraping", "smtp_verification").
    pub methods_used: Vec<String>,
    /// A log of verification attempts and their outcomes for specific emails.
    pub verification_log: HashMap<String, String>,
}

/// Represents the final output structure for each record, combining input and results.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ProcessingResult {
    // Include all fields from the original Contact input
    #[serde(flatten)]
    pub contact_input: Contact,

    // Fields added during processing
    /// The results of the email discovery process. Nested structure.
    pub email_discovery_results: Option<EmailResult>, // Optional in case of skipping/errors before discovery
    /// The primary email found (convenience field, mirrors EmailResult.most_likely_email).
    #[serde(skip_serializing_if = "Option::is_none")] // Don't write if None
    pub email: Option<String>,
    /// Confidence score for the primary email (convenience field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_confidence: Option<u8>,
    /// A comma-separated list of methods used (convenience field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verification_method: Option<String>,
    /// List of alternative emails found (convenience field).
    #[serde(skip_serializing_if = "Vec::is_empty")] // Don't write if empty
    #[serde(default)] // Needed if skip_serializing_if is used
    pub email_alternatives: Vec<String>,

    // Status/Error fields
    /// Flag indicating if the record was skipped due to missing input.
    #[serde(skip_serializing_if = "std::ops::Not::not")] // Don't write if false
    #[serde(default)]
    pub email_finding_skipped: bool,
    /// Reason why the record was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_finding_reason: Option<String>,
    /// Flag indicating verification failed definitively for the top choices.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[serde(default)]
    pub email_verification_failed: bool, // Set if no likely email found/verified
    /// Error message if processing failed unexpectedly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_finding_error: Option<String>,
}

/// Internal representation after validating input Contact
#[derive(Debug, Clone)]
pub(crate) struct ValidatedContact {
    pub first_name: String,
    pub last_name: String,
    /// Guaranteed to be populated (either from input or constructed).
    pub full_name: String,
    /// The base URL derived from the input domain, used for scraping.
    pub website_url: Url,
    /// The extracted, lowercase domain name used for patterns and verification.
    pub domain: String,
    // Keep original contact for outputting all original fields.
    pub original_contact: Contact,
}

/// Internal representation of SMTP verification outcome
#[derive(Debug, Clone)]
pub(crate) struct SmtpVerificationResult {
    /// True = Exists, False = Does Not Exist, None = Inconclusive/Error
    pub exists: Option<bool>,
    /// Detailed message about the outcome.
    pub message: String,
    /// Suggests if retrying might yield a different result (e.g., for temporary errors).
    pub should_retry: bool,
    /// Indicates if the domain seems to accept all emails.
    pub is_catch_all: bool,
}

impl SmtpVerificationResult {
    /// Creates a conclusive result (email definitely exists or not).
    pub(crate) fn conclusive(exists: bool, message: String, is_catch_all: bool) -> Self {
        Self {
            exists: Some(exists),
            message,
            should_retry: false,
            is_catch_all,
        }
    }

    /// Creates an inconclusive result where retrying might help.
    pub(crate) fn inconclusive_retry(message: String) -> Self {
        Self {
            exists: None,
            message,
            should_retry: true,
            is_catch_all: false,
        }
    }

    /// Creates an inconclusive result where retrying is unlikely to help.
    pub(crate) fn inconclusive_no_retry(message: String) -> Self {
        Self {
            exists: None,
            message,
            should_retry: false,
            is_catch_all: false,
        }
    }
}
