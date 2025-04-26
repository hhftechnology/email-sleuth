//! Functions for performing DNS lookups (MX, A records).

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use trust_dns_resolver::TokioAsyncResolver;
use trust_dns_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};

/// Represents the result of a mail server lookup.
#[derive(Debug, Clone)]
pub(crate) struct MailServer {
    /// The domain name or IP address of the mail server.
    pub exchange: String,
    /// The preference value (lower is more preferred), typically from MX records.
    /// Will be `u16::MAX` if derived from an A record.
    pub preference: u16,
}

/// Creates a configured DNS resolver instance.
pub(crate) async fn create_resolver() -> Result<TokioAsyncResolver> {
    let mut resolver_config = ResolverConfig::new();

    for server_str in &CONFIG.dns_servers {
        match IpAddr::from_str(server_str) {
            Ok(ip_addr) => {
                // Default DNS port is 53
                let socket_addr = SocketAddr::new(ip_addr, 53);
                resolver_config.add_name_server(trust_dns_resolver::config::NameServerConfig {
                    socket_addr,
                    protocol: trust_dns_resolver::config::Protocol::Udp, // Start with UDP
                    tls_dns_name: None,
                    trust_negative_responses: true,
                    bind_addr: None,
                });
                resolver_config.add_name_server(trust_dns_resolver::config::NameServerConfig {
                    socket_addr,
                    protocol: trust_dns_resolver::config::Protocol::Tcp, // Also allow TCP fallback
                    tls_dns_name: None,
                    trust_negative_responses: true,
                    bind_addr: None,
                });
            }
            Err(e) => {
                tracing::error!(
                    "Invalid DNS server IP address in config: '{}' - {}",
                    server_str,
                    e
                );
                return Err(AppError::Config(format!(
                    "Invalid DNS server IP address: {}",
                    server_str
                )));
            }
        }
    }

    let mut resolver_opts = ResolverOpts::default();
    resolver_opts.timeout = CONFIG.dns_timeout;
    resolver_opts.attempts = 2;
    resolver_opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;

    let resolver = TokioAsyncResolver::tokio(resolver_config, resolver_opts);
    tracing::debug!("DNS resolver configured with public servers and timeout.");
    Ok(resolver)
}

/// Resolves the mail server(s) for a given domain, checking MX records first,
/// then falling back to A records.
///
/// # Arguments
/// * `resolver` - A configured `TokioAsyncResolver` instance.
/// * `domain` - The domain name to resolve.
///
/// # Returns
/// * `Ok(MailServer)` containing the most preferred mail server found.
/// * `Err(AppError)` if resolution fails (e.g., NXDOMAIN, NoAnswer, Timeout).
pub(crate) async fn resolve_mail_server(
    resolver: &TokioAsyncResolver,
    domain: &str,
) -> Result<MailServer> {
    tracing::debug!("Performing DNS MX lookup for {}", domain);

    match resolver.mx_lookup(domain).await {
        Ok(mx_response) => {
            let mut mx_records: Vec<_> = mx_response.iter().collect();
            if mx_records.is_empty() {
                tracing::warn!(
                    "No MX records returned by resolver for {}, though lookup succeeded.",
                    domain
                );
                return resolve_a_record_fallback(resolver, domain).await;
            }

            mx_records.sort_by_key(|r| r.preference());

            if let Some(best_mx) = mx_records.first() {
                let exchange = best_mx
                    .exchange()
                    .to_utf8()
                    .trim_end_matches('.')
                    .to_string();
                let preference = best_mx.preference();
                if exchange.is_empty() {
                    tracing::error!(
                        "Empty mail server name found in highest priority MX record for {}",
                        domain
                    );
                    return Err(AppError::NoDnsRecords(format!(
                        "Empty exchange in MX record for {}",
                        domain
                    )));
                }
                tracing::info!(
                    "Found MX for {}: {} (Pref: {})",
                    domain,
                    exchange,
                    preference
                );
                Ok(MailServer {
                    exchange,
                    preference,
                })
            } else {
                tracing::warn!(
                    "MX lookup for {} succeeded but yielded no processable records.",
                    domain
                );
                resolve_a_record_fallback(resolver, domain).await
            }
        }
        Err(e) => {
            let error_string = format!("{:?}", e.kind());

            if error_string.contains("NoRecordsFound") {
                tracing::warn!(
                    "No MX records found (NoAnswer) for {}. Trying A record fallback...",
                    domain
                );
                resolve_a_record_fallback(resolver, domain).await
            } else if error_string.contains("NXDomain")
                || error_string.contains("Name does not exist")
            {
                tracing::error!("Domain {} does not exist (NXDOMAIN)", domain);
                Err(AppError::NxDomain(domain.to_string()))
            } else if error_string.contains("Timeout") {
                tracing::error!("DNS resolution timeout for {}", domain);
                Err(AppError::DnsTimeout(domain.to_string()))
            } else {
                tracing::error!("Unexpected DNS resolution error for {}: {}", domain, e);
                Err(AppError::Dns(e))
            }
        }
    }
}

/// Attempts to resolve an A record for the domain as a fallback mail server.
async fn resolve_a_record_fallback(
    resolver: &TokioAsyncResolver,
    domain: &str,
) -> Result<MailServer> {
    tracing::debug!("Attempting A record fallback for {}", domain);
    match resolver.lookup_ip(domain).await {
        Ok(a_response) => {
            if let Some(ip_addr) = a_response.iter().next() {
                let mail_server_ip = ip_addr.to_string();
                tracing::info!(
                    "Using A record for {} as mail server: {}",
                    domain,
                    mail_server_ip
                );
                Ok(MailServer {
                    exchange: mail_server_ip,
                    preference: u16::MAX,
                })
            } else {
                tracing::error!("No MX or A records found for {}", domain);
                Err(AppError::NoDnsRecords(domain.to_string()))
            }
        }
        Err(e) => {
            let error_string = format!("{:?}", e.kind());

            if error_string.contains("NoRecordsFound") {
                tracing::error!(
                    "No MX records found, and no A records found either for {}",
                    domain
                );
                Err(AppError::NoDnsRecords(domain.to_string()))
            } else if error_string.contains("NXDomain")
                || error_string.contains("Name does not exist")
            {
                tracing::error!(
                    "Domain {} does not exist (NXDOMAIN) during A record fallback",
                    domain
                );
                Err(AppError::NxDomain(domain.to_string()))
            } else if error_string.contains("Timeout") {
                tracing::error!("DNS timeout during A record fallback for {}", domain);
                Err(AppError::DnsTimeout(format!(
                    "A record fallback for {}",
                    domain
                )))
            } else {
                tracing::error!(
                    "A record fallback failed for {} after NoAnswer MX: {}",
                    domain,
                    e
                );
                Err(AppError::Dns(e))
            }
        }
    }
}
