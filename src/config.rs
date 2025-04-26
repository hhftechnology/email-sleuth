//! Defines the configuration settings for the email-sleuth application.

use anyhow::Context;
use clap::Parser;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Command line arguments for email-sleuth
#[derive(Parser, Debug)]
#[command(author, version, about = "A Rust application to discover and verify professional email addresses", long_about = None)]
pub(crate) struct AppArgs {
    /// Path to the input JSON file containing contacts
    #[arg(short, long, default_value = "input.json", env = "EMAIL_SLEUTH_INPUT")]
    pub input: String,

    /// Path to the output JSON file where results will be saved
    #[arg(
        short,
        long,
        default_value = "results.json",
        env = "EMAIL_SLEUTH_OUTPUT"
    )]
    pub output: String,

    /// Maximum number of concurrent tasks
    #[arg(short, long, env = "EMAIL_SLEUTH_CONCURRENCY")]
    pub concurrency: Option<usize>,

    /// Name of the person to find email for (CLI mode)
    #[arg(long, env = "EMAIL_SLEUTH_NAME")]
    pub name: Option<String>,

    /// Domain to search against (CLI mode)
    #[arg(long, env = "EMAIL_SLEUTH_DOMAIN")]
    pub domain: Option<String>,

    /// Output results to stdout in CLI mode
    #[arg(long, default_value = "false", env = "EMAIL_SLEUTH_STDOUT")]
    pub stdout: bool,

    /// Path to configuration file (TOML format)
    #[arg(long, env = "EMAIL_SLEUTH_CONFIG")]
    pub config_file: Option<String>,

    /// Maximum number of SMTP verification attempts
    #[arg(long, env = "EMAIL_SLEUTH_MAX_VERIFICATION_ATTEMPTS")]
    pub max_verification_attempts: Option<u32>,

    /// Minimum sleep between requests (seconds)
    #[arg(long, env = "EMAIL_SLEUTH_MIN_SLEEP")]
    pub min_sleep: Option<f32>,

    /// Maximum sleep between requests (seconds)
    #[arg(long, env = "EMAIL_SLEUTH_MAX_SLEEP")]
    pub max_sleep: Option<f32>,

    /// HTTP request timeout in seconds
    #[arg(long, env = "EMAIL_SLEUTH_REQUEST_TIMEOUT")]
    pub request_timeout: Option<u64>,

    /// SMTP connection timeout in seconds
    #[arg(long, env = "EMAIL_SLEUTH_SMTP_TIMEOUT")]
    pub smtp_timeout: Option<u64>,

    /// DNS resolution timeout in seconds
    #[arg(long, env = "EMAIL_SLEUTH_DNS_TIMEOUT")]
    pub dns_timeout: Option<u64>,

    /// Comma-separated list of DNS servers
    #[arg(long, env = "EMAIL_SLEUTH_DNS_SERVERS")]
    pub dns_servers: Option<String>,

    /// Comma-separated list of common pages to scrape
    #[arg(long, env = "EMAIL_SLEUTH_COMMON_PAGES")]
    pub common_pages: Option<String>,

    /// User agent string for HTTP requests
    #[arg(long, env = "EMAIL_SLEUTH_USER_AGENT")]
    pub user_agent: Option<String>,

    /// Sender email address for SMTP verification
    #[arg(long, env = "EMAIL_SLEUTH_SMTP_SENDER")]
    pub smtp_sender: Option<String>,

    /// Base confidence threshold score (0-10)
    #[arg(long, env = "EMAIL_SLEUTH_CONFIDENCE_THRESHOLD")]
    pub confidence_threshold: Option<u8>,

    /// Generic email confidence threshold score (0-10)
    #[arg(long, env = "EMAIL_SLEUTH_GENERIC_CONFIDENCE_THRESHOLD")]
    pub generic_confidence_threshold: Option<u8>,

    /// Maximum number of alternative emails to list
    #[arg(long, env = "EMAIL_SLEUTH_MAX_ALTERNATIVES")]
    pub max_alternatives: Option<usize>,
}

/// TOML Configuration file structure
#[derive(Deserialize, Debug, Default)]
struct ConfigFile {
    network: Option<NetworkConfig>,
    dns: Option<DnsConfig>,
    smtp: Option<SmtpConfig>,
    scraping: Option<ScrapingConfig>,
    verification: Option<VerificationConfig>,
    input_output: Option<InputOutputConfig>,
}

#[derive(Deserialize, Debug, Default)]
struct NetworkConfig {
    request_timeout: Option<u64>,
    min_sleep: Option<f32>,
    max_sleep: Option<f32>,
    user_agent: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct DnsConfig {
    dns_timeout: Option<u64>,
    dns_servers: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Default)]
struct SmtpConfig {
    smtp_timeout: Option<u64>,
    smtp_sender_email: Option<String>,
    max_verification_attempts: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct ScrapingConfig {
    common_pages: Option<Vec<String>>,
    generic_email_prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Default)]
struct VerificationConfig {
    confidence_threshold: Option<u8>,
    generic_confidence_threshold: Option<u8>,
    max_alternatives: Option<usize>,
    max_concurrency: Option<usize>,
}

#[derive(Deserialize, Debug, Default)]
struct InputOutputConfig {
    input_file: Option<String>,
    output_file: Option<String>,
}

/// Application configuration settings.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// Path to the input JSON file containing contacts.
    pub input_file: String,
    /// Path to the output JSON file where results will be saved.
    pub output_file: String,
    /// Maximum number of concurrent tasks (e.g., processing contacts).
    pub max_concurrency: usize,
    /// Maximum number of SMTP verification attempts for an inconclusive email.
    pub max_verification_attempts: u32,
    /// Minimum and maximum sleep duration between HTTP requests (seconds).
    pub sleep_between_requests: (f32, f32),
    /// Timeout for individual HTTP requests.
    pub request_timeout: Duration,
    /// Timeout for establishing SMTP connections and individual commands.
    pub smtp_timeout: Duration,
    /// Timeout for DNS resolution queries.
    pub dns_timeout: Duration,
    /// Common sub-pages to check for contact information during scraping.
    pub common_pages_to_scrape: Vec<String>,
    /// Regex pattern for matching email addresses.
    pub email_regex: Regex,
    /// Set of common generic email prefixes (e.g., "info", "contact").
    pub generic_email_prefixes: HashSet<String>,
    /// User agent string to use for HTTP requests.
    pub user_agent: String,
    /// Sender email address to use in the SMTP MAIL FROM command.
    pub smtp_sender_email: String,
    /// DNS servers to use for resolution.
    pub dns_servers: Vec<String>,
    /// Confidence score threshold to select an email as "most likely".
    pub confidence_threshold: u8,
    /// Confidence score above which a generic email might be selected as "most likely".
    pub generic_confidence_threshold: u8,
    /// Maximum number of alternative emails to list in the output.
    pub max_alternatives: usize,
    /// Flag indicating if the application is running in CLI mode (processing a single contact).
    pub cli_mode: bool,
    /// The name provided via command line when running in CLI mode.
    pub cli_name: Option<String>,
    /// The domain provided via command line when running in CLI mode.
    pub cli_domain: Option<String>,
    /// Flag to output results to stdout instead of a file in CLI mode.
    pub output_to_stdout: bool,
}

impl Config {
    fn default() -> Self {
        let common_pages = vec![
            "/contact",
            "/contact-us",
            "/contactus",
            "/contact_us",
            "/about",
            "/about-us",
            "/aboutus",
            "/about_us",
            "/team",
            "/our-team",
            "/our_team",
            "/meet-the-team",
            "/people",
            "/staff",
            "/company",
        ];

        let generic_prefixes: HashSet<String> = [
            "info",
            "contact",
            "hello",
            "help",
            "support",
            "admin",
            "office",
            "sales",
            "press",
            "media",
            "marketing",
            "jobs",
            "careers",
            "hiring",
            "privacy",
            "security",
            "legal",
            "membership",
            "team",
            "people",
            "general",
            "feedback",
            "enquiries",
            "inquiries",
            "mail",
            "email",
            "pitch",
            "invest",
            "investors",
            "ir",
            "webmaster",
            "newsletter",
            "apply",
            "partner",
            "partners",
            "ventures",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let email_regex_pattern = r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b";
        let email_regex = Regex::new(email_regex_pattern)
            .expect("Failed to compile email regex pattern. This should not happen.");

        let dns_servers = vec![
            "8.8.8.8".to_string(),
            "8.8.4.4".to_string(),
            "1.1.1.1".to_string(),
            "1.0.0.1".to_string(),
        ];

        Config {
            input_file: "input.json".to_string(),
            output_file: "results.json".to_string(),
            max_concurrency: 8,
            max_verification_attempts: 2,
            sleep_between_requests: (0.1, 0.5),
            request_timeout: Duration::from_secs(10),
            smtp_timeout: Duration::from_secs(5),
            dns_timeout: Duration::from_secs(5),
            common_pages_to_scrape: common_pages.iter().map(|s| s.to_string()).collect(),
            email_regex,
            generic_email_prefixes: generic_prefixes,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/118.0.0.0 Safari/537.36".to_string(),
            smtp_sender_email: "verify-probe@example.com".to_string(),
            dns_servers,
            confidence_threshold: 4,
            generic_confidence_threshold: 7,
            max_alternatives: 5,
            cli_mode: false,
            cli_name: None,
            cli_domain: None,
            output_to_stdout: false,
        }
    }
}

/// Load configuration from a TOML file
fn load_config_file(file_path: &str) -> anyhow::Result<ConfigFile> {
    let path = Path::new(file_path);
    if !path.exists() {
        tracing::warn!("Configuration file {} not found, using defaults", file_path);
        return Ok(ConfigFile::default());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read configuration file: {}", file_path))?;

    let config: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML configuration from {}", file_path))?;

    tracing::info!("Loaded configuration from {}", file_path);
    Ok(config)
}

fn apply_file_config(config: &mut Config, file_config: &ConfigFile) {
    if let Some(network) = &file_config.network {
        if let Some(timeout) = network.request_timeout {
            config.request_timeout = Duration::from_secs(timeout);
        }
        if let Some(min_sleep) = network.min_sleep {
            config.sleep_between_requests.0 = min_sleep;
        }
        if let Some(max_sleep) = network.max_sleep {
            config.sleep_between_requests.1 = max_sleep;
        }
        if let Some(user_agent) = &network.user_agent {
            config.user_agent = user_agent.clone();
        }
    }

    if let Some(dns) = &file_config.dns {
        if let Some(timeout) = dns.dns_timeout {
            config.dns_timeout = Duration::from_secs(timeout);
        }
        if let Some(servers) = &dns.dns_servers {
            config.dns_servers = servers.clone();
        }
    }

    if let Some(smtp) = &file_config.smtp {
        if let Some(timeout) = smtp.smtp_timeout {
            config.smtp_timeout = Duration::from_secs(timeout);
        }
        if let Some(sender) = &smtp.smtp_sender_email {
            config.smtp_sender_email = sender.clone();
        }
        if let Some(attempts) = smtp.max_verification_attempts {
            config.max_verification_attempts = attempts;
        }
    }

    if let Some(scraping) = &file_config.scraping {
        if let Some(pages) = &scraping.common_pages {
            config.common_pages_to_scrape = pages.clone();
        }
        if let Some(prefixes) = &scraping.generic_email_prefixes {
            config.generic_email_prefixes = prefixes.iter().map(|s| s.clone()).collect();
        }
    }

    if let Some(verification) = &file_config.verification {
        if let Some(threshold) = verification.confidence_threshold {
            config.confidence_threshold = threshold;
        }
        if let Some(gen_threshold) = verification.generic_confidence_threshold {
            config.generic_confidence_threshold = gen_threshold;
        }
        if let Some(max_alt) = verification.max_alternatives {
            config.max_alternatives = max_alt;
        }
        if let Some(concurrency) = verification.max_concurrency {
            config.max_concurrency = concurrency;
        }
    }

    if let Some(io_config) = &file_config.input_output {
        if let Some(input) = &io_config.input_file {
            config.input_file = input.clone();
        }
        if let Some(output) = &io_config.output_file {
            config.output_file = output.clone();
        }
    }
}

/// Apply command line arguments to the Config instance
fn apply_cli_args(config: &mut Config, args: &AppArgs) {
    config.input_file = args.input.clone();
    config.output_file = args.output.clone();

    config.cli_name = args.name.clone();
    config.cli_domain = args.domain.clone();
    config.cli_mode = args.name.is_some() && args.domain.is_some();
    config.output_to_stdout = args.stdout;

    if let Some(concurrency) = args.concurrency {
        config.max_concurrency = concurrency;
    }

    if let Some(attempts) = args.max_verification_attempts {
        config.max_verification_attempts = attempts;
    }

    if let Some(min_sleep) = args.min_sleep {
        config.sleep_between_requests.0 = min_sleep;
    }

    if let Some(max_sleep) = args.max_sleep {
        config.sleep_between_requests.1 = max_sleep;
    }

    if let Some(timeout) = args.request_timeout {
        config.request_timeout = Duration::from_secs(timeout);
    }

    if let Some(timeout) = args.smtp_timeout {
        config.smtp_timeout = Duration::from_secs(timeout);
    }

    if let Some(timeout) = args.dns_timeout {
        config.dns_timeout = Duration::from_secs(timeout);
    }

    if let Some(ref servers) = args.dns_servers {
        config.dns_servers = servers
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Some(ref pages) = args.common_pages {
        config.common_pages_to_scrape = pages
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Some(ref agent) = args.user_agent {
        config.user_agent = agent.clone();
    }

    if let Some(ref sender) = args.smtp_sender {
        config.smtp_sender_email = sender.clone();
    }

    if let Some(threshold) = args.confidence_threshold {
        config.confidence_threshold = threshold;
    }

    if let Some(threshold) = args.generic_confidence_threshold {
        config.generic_confidence_threshold = threshold;
    }

    if let Some(max_alt) = args.max_alternatives {
        config.max_alternatives = max_alt;
    }
}

fn validate_config(config: &mut Config) -> anyhow::Result<()> {
    if config.sleep_between_requests.0 > config.sleep_between_requests.1 {
        config.sleep_between_requests.1 = config.sleep_between_requests.0;
        tracing::warn!(
            "Min sleep was greater than max sleep. Setting both to {}",
            config.sleep_between_requests.0
        );
    }

    if config.dns_servers.is_empty() {
        config.dns_servers = vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()];
        tracing::warn!("DNS servers list was empty. Setting to default public DNS servers.");
    }

    if config.confidence_threshold > 10 {
        config.confidence_threshold = 10;
        tracing::warn!("Confidence threshold exceeded maximum (10). Setting to 10.");
    }

    if config.generic_confidence_threshold > 10 {
        config.generic_confidence_threshold = 10;
        tracing::warn!("Generic confidence threshold exceeded maximum (10). Setting to 10.");
    }

    if config.generic_confidence_threshold < config.confidence_threshold {
        config.generic_confidence_threshold = config.confidence_threshold;
        tracing::warn!(
            "Generic confidence threshold was less than base threshold. Setting to {}",
            config.confidence_threshold
        );
    }

    if config.max_concurrency == 0 {
        config.max_concurrency = 1;
        tracing::warn!("Concurrency was set to 0. Setting to 1.");
    }

    Ok(())
}

pub(crate) fn build_config() -> anyhow::Result<Config> {
    let args = AppArgs::parse();

    let mut config = Config::default();

    if let Some(ref file_path) = args.config_file {
        match load_config_file(file_path) {
            Ok(file_config) => apply_file_config(&mut config, &file_config),
            Err(e) => {
                tracing::error!("Failed to load configuration file: {}", e);
            }
        }
    } else {
        for path in [
            "./email-sleuth.toml",
            "./config.toml",
            "~/.config/email-sleuth.toml",
        ]
        .iter()
        {
            if Path::new(path).exists() {
                match load_config_file(path) {
                    Ok(file_config) => {
                        apply_file_config(&mut config, &file_config);
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load configuration from {}: {}", path, e);
                    }
                }
            }
        }
    }

    apply_cli_args(&mut config, &args);

    validate_config(&mut config)?;

    tracing::debug!("Final configuration: {:?}", config);

    Ok(config)
}

pub(crate) fn parse_args() -> AppArgs {
    AppArgs::parse()
}

pub(crate) static CONFIG: Lazy<Config> = Lazy::new(|| match build_config() {
    Ok(config) => config,
    Err(e) => {
        eprintln!("ERROR: Failed to build configuration: {}", e);
        Config::default()
    }
});

pub(crate) fn get_random_sleep_duration() -> Duration {
    use rand::Rng;
    let (min, max) = CONFIG.sleep_between_requests;
    if min >= max {
        return Duration::from_secs_f32(min);
    }
    let duration_secs = rand::thread_rng().gen_range(min..max);
    Duration::from_secs_f32(duration_secs)
}
