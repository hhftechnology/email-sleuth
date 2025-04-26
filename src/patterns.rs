//! Generates potential email address patterns based on names and domain.

use crate::config::CONFIG;
use std::collections::HashSet;

/// Removes whitespace and converts to lowercase.
fn sanitize_name_part(part: &str) -> String {
    part.trim().replace(char::is_whitespace, "").to_lowercase()
}

/// Generates a list of common email address patterns for a given name and domain.
///
/// # Arguments
/// * `first_name` - The contact's first name.
/// * `last_name` - The contact's last name.
/// * `domain` - The company domain name (e.g., "example.com").
///
/// # Returns
/// * `Vec<String>` containing potential email patterns. Returns an empty vector if
///   names or domain are empty or invalid.
pub(crate) fn generate_email_patterns(
    first_name: &str,
    last_name: &str,
    domain: &str,
) -> Vec<String> {
    tracing::debug!(
        "Generating patterns for {} {} @ {}",
        first_name,
        last_name,
        domain
    );

    if first_name.is_empty() || last_name.is_empty() || domain.is_empty() || !domain.contains('.') {
        tracing::warn!(
            "Cannot generate patterns due to empty name/domain or invalid domain: '{} {} @ {}'",
            first_name,
            last_name,
            domain
        );
        return Vec::new();
    }

    let first = sanitize_name_part(first_name);
    let last = sanitize_name_part(last_name);

    if first.is_empty() || last.is_empty() {
        tracing::warn!(
            "Cannot generate patterns after sanitizing names: '{} {} @ {}'",
            first,
            last,
            domain
        );
        return Vec::new();
    }

    let first_initial = first.chars().next().unwrap_or_default();
    let last_initial = last.chars().next().unwrap_or_default();

    // Use a HashSet to automatically handle duplicates
    let mut patterns = HashSet::new();

    patterns.insert(format!("{}@{}", first, domain)); // john@domain.com
    patterns.insert(format!("{}.{}@{}", first, last, domain)); // john.doe@domain.com
    patterns.insert(format!("{}{}", first, last)); // johndoe@domain.com - Base part only
    patterns.insert(format!("{}.{}@{}", last, first, domain)); // doe.john@domain.com
    patterns.insert(format!("{}{}", last, first)); // doejohn@domain.com - Base part only
    patterns.insert(format!("{}{}", first_initial, last)); // jdoe@domain.com - Base part only
    patterns.insert(format!("{}.{}@{}", first_initial, last, domain)); // j.doe@domain.com
    patterns.insert(format!("{}{}", first, last_initial)); // johnd@domain.com - Base part only
    patterns.insert(format!("{}.{}@{}", first, last_initial, domain)); // john.d@domain.com
    patterns.insert(format!("{}_{}@{}", first, last, domain)); // john_doe@domain.com
    patterns.insert(format!("{}-{}@{}", first, last, domain)); // john-doe@domain.com
    patterns.insert(format!("{}_{}@{}", last, first, domain)); // doe_john@domain.com
    patterns.insert(format!("{}-{}@{}", last, first, domain)); // doe-john@domain.com

    if first.len() >= 3 {
        patterns.insert(format!("{}{}", &first[0..3], last));
    }
    if last.len() >= 3 {
        patterns.insert(format!("{}{}", first, &last[0..3]));
    }

    let final_patterns: Vec<String> = patterns
        .iter()
        .map(|p| {
            if !p.contains('@') {
                format!("{}@{}", p, domain)
            } else {
                p.clone()
            }
        })
        .filter(|p| CONFIG.email_regex.is_match(p))
        .collect();

    tracing::debug!("Generated {} unique valid patterns.", final_patterns.len());
    final_patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_patterns_basic() {
        let patterns = generate_email_patterns("John", "Doe", "example.com");
        assert!(!patterns.is_empty());
        assert!(patterns.contains(&"john.doe@example.com".to_string()));
        assert!(patterns.contains(&"jdoe@example.com".to_string()));
        assert!(patterns.contains(&"john@example.com".to_string()));
        assert!(patterns.contains(&"doe.john@example.com".to_string()));
        assert!(patterns.contains(&"johnd@example.com".to_string()));
        assert!(patterns.contains(&"john_doe@example.com".to_string()));
    }

    #[test]
    fn test_generate_patterns_with_spaces() {
        let patterns = generate_email_patterns(" John ", " Van Der Beek ", "test.co.uk");
        assert!(patterns.contains(&"john.vanderbeek@test.co.uk".to_string()));
        assert!(patterns.contains(&"jvanderbeek@test.co.uk".to_string()));
        assert!(patterns.contains(&"johnv@test.co.uk".to_string())); // From john.v@... pattern
    }

    #[test]
    fn test_generate_patterns_empty_input() {
        assert!(generate_email_patterns("", "Doe", "example.com").is_empty());
        assert!(generate_email_patterns("John", "", "example.com").is_empty());
        assert!(generate_email_patterns("John", "Doe", "").is_empty());
        assert!(generate_email_patterns("John", "Doe", "nodot").is_empty()); // Invalid domain
        assert!(generate_email_patterns(" ", "Doe", "example.com").is_empty()); // Sanitized name becomes empty
    }

    #[test]
    fn test_generate_patterns_duplicates() {
        // Example: If first = "test" and last = "test"
        let patterns = generate_email_patterns("Test", "Test", "test.com");
        let count_test_test = patterns
            .iter()
            .filter(|&p| p == "test.test@test.com")
            .count();
        assert_eq!(count_test_test, 1, "Duplicate patterns should be removed");
        let count_ttest = patterns.iter().filter(|&p| p == "ttest@test.com").count();
        assert_eq!(count_ttest, 1, "Duplicate patterns should be removed");
    }
}
