//! Functions for verifying email address existence via SMTP.

use crate::config::{CONFIG, get_random_sleep_duration};
use crate::error::{AppError, Result};
use crate::models::SmtpVerificationResult;
use lettre::Address;
use lettre::transport::smtp::client::SmtpConnection;
use lettre::transport::smtp::commands::{Ehlo, Mail, Rcpt};
use lettre::transport::smtp::response::{Code, Severity};
use rand::Rng;
use std::net::ToSocketAddrs;
use std::str::FromStr;
use std::time::Duration;

/// Performs the SMTP RCPT TO check for a single email address.
/// This attempts to replicate the logic from the Python script's _verify_smtp function.
/// Uses lower-level SmtpConnection for command control.
///
/// # Arguments
/// * `email` - The email address to verify.
/// * `domain` - The domain part of the email address.
/// * `mail_server` - The hostname or IP address of the mail server obtained via DNS.
///
/// # Returns
/// * `Result<SmtpVerificationResult>` indicating whether the email likely exists,
///   doesn't exist, or if the check was inconclusive.
async fn verify_smtp_email(
    email: &str,
    domain: &str,
    mail_server: &str,
) -> Result<SmtpVerificationResult> {
    tracing::debug!(target: "smtp_task",
        "Starting SMTP check for {} via {} (Domain: {})",
        email,
        mail_server,
        domain
    );

    let recipient_address = match Address::from_str(email) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!(target: "smtp_task", "Invalid recipient email format '{}': {}", email, e);
            return Ok(SmtpVerificationResult::conclusive(
                false,
                format!("Invalid email format: {}", e),
                false,
            ));
        }
    };

    let sender_address = Address::from_str(&CONFIG.smtp_sender_email)
        .map_err(|e| AppError::Config(format!("Invalid sender email in config: {}", e)))?;

    let socket_addr = match (mail_server, 25_u16).to_socket_addrs()?.next() {
        Some(addr) => addr,
        None => {
            tracing::error!(target: "smtp_task", "Could not resolve mail server address: {}", mail_server);
            return Ok(SmtpVerificationResult::inconclusive_no_retry(format!(
                "Could not resolve mail server address: {}",
                mail_server
            )));
        }
    };

    let helo_name = lettre::transport::smtp::extension::ClientId::Domain("localhost".to_string());

    let mut smtp_conn = match SmtpConnection::connect(
        socket_addr,
        Some(CONFIG.smtp_timeout),
        &helo_name,
        None,
        None,
    ) {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(target: "smtp_task", "SMTP connection failed for {}: {}", mail_server, e);

            let err_string = e.to_string();
            if err_string.contains("timed out") || err_string.contains("connection refused") {
                tracing::error!(target: "smtp_task", 
                    "Port 25 appears to be blocked by your ISP or network. Consider using a different network or VPN.");
                return Ok(SmtpVerificationResult::inconclusive_no_retry(
                    "Port 25 is likely blocked by your ISP. Try using a different network or VPN."
                        .to_string(),
                ));
            }

            return Ok(handle_smtp_error(&e, mail_server));
        }
    };

    match smtp_conn.command(Ehlo::new(helo_name.clone())) {
        Ok(_) => {
            tracing::debug!(target: "smtp_task", "Initial EHLO successful");
        }
        Err(e) => {
            tracing::warn!(target: "smtp_task", "Initial EHLO failed: {}", e);
            return Ok(handle_smtp_error(&e, mail_server));
        }
    }

    tracing::debug!(target: "smtp_task", "SMTP connection established to {}:{}", mail_server, socket_addr.port());

    tracing::debug!(target: "smtp_task", "Sending MAIL FROM:<{}>...", &CONFIG.smtp_sender_email);
    match smtp_conn.command(Mail::new(Some(sender_address.clone()), vec![])) {
        Ok(response) => {
            if response.is_positive() {
                tracing::debug!(target: "smtp_task", "MAIL FROM accepted by {}: {:?}", mail_server, response);
            } else {
                tracing::error!(target: "smtp_task",
                    "SMTP sender '{}' rejected by {}: {:?}",
                    &CONFIG.smtp_sender_email, mail_server, response
                );
                smtp_conn.quit().ok();
                return Ok(SmtpVerificationResult::inconclusive_no_retry(format!(
                    "MAIL FROM rejected: {} {}",
                    response.code(),
                    response.message().collect::<Vec<&str>>().join(" ")
                )));
            }
        }
        Err(e) => {
            tracing::error!(target: "smtp_task", "Error during MAIL FROM on {}: {}", mail_server, e);
            smtp_conn.quit().ok();
            return Ok(handle_smtp_error(&e, mail_server));
        }
    }

    tracing::debug!(target: "smtp_task", "Sending RCPT TO:<{}>...", email);
    let rcpt_result = smtp_conn.command(Rcpt::new(recipient_address.clone(), vec![]));

    let (target_code, target_message): (Code, String) = match rcpt_result {
        Ok(response) => (
            response.code(),
            response.message().collect::<Vec<&str>>().join(" "),
        ),
        Err(e) => {
            let err_string = e.to_string();

            let is_nonexistent_error = err_string.contains("550")
                && (err_string.contains("does not exist")
                    || err_string.contains("no such user")
                    || err_string.contains("user unknown")
                    || err_string.contains("recipient not found")
                    || err_string.contains("NoSuchUser"));

            if is_nonexistent_error {
                tracing::info!(target: "smtp_task", 
                    "Email not found during check for {} on {}: {}", 
                    email, mail_server, e);
            } else {
                tracing::error!(target: "smtp_task", 
                    "Error during RCPT TO for {} on {}: {}", 
                    email, mail_server, e);
            }

            smtp_conn.quit().ok();
            return Ok(handle_smtp_error(&e, mail_server));
        }
    };

    tracing::info!(target: "smtp_task",
        "RCPT TO:<{}> result: Code={}, Msg='{}'",
        email, target_code, target_message
    );

    let mut is_catch_all = false;
    if target_code.severity == Severity::PositiveCompletion {
        let random_user = format!(
            "no-reply-does-not-exist-{}@{}",
            rand::thread_rng().gen_range(100000..999999),
            domain
        );
        match Address::from_str(&random_user) {
            Ok(random_address) => {
                tracing::debug!(target: "smtp_task", "Checking for catch-all with: RCPT TO:<{}>", random_user);
                match smtp_conn.command(Rcpt::new(random_address, vec![])) {
                    Ok(response) if response.code().severity == Severity::PositiveCompletion => {
                        is_catch_all = true;
                        tracing::warn!(target: "smtp_task",
                            "Domain {} appears to be a catch-all (accepted random user {} with code {})",
                            domain, random_user, response.code()
                        );
                    }
                    Ok(response) => {
                        tracing::debug!(target: "smtp_task",
                            "Catch-all check negative (random user rejected with code {})", response.code()
                        );
                    }
                    Err(e) => {
                        tracing::warn!(target: "smtp_task", "Error during catch-all RCPT TO check (ignoring): {}", e);
                    }
                }
            }
            Err(_) => {
                tracing::error!(target: "smtp_task", "Failed to parse generated random email for catch-all check: {}", random_user);
            }
        }
    }

    let final_result = match target_code.severity {
        Severity::PositiveCompletion => {
            if is_catch_all {
                SmtpVerificationResult::inconclusive_retry(format!(
                    "SMTP accepted (Possible Catch-All): {} {}",
                    target_code, target_message
                ))
            } else {
                SmtpVerificationResult::conclusive(
                    true,
                    format!("SMTP Verification OK: {} {}", target_code, target_message),
                    false,
                )
            }
        }
        Severity::PositiveIntermediate => SmtpVerificationResult::inconclusive_retry(format!(
            "SMTP Unexpected Intermediate Code: {} {}",
            target_code, target_message
        )),
        Severity::TransientNegativeCompletion => {
            SmtpVerificationResult::inconclusive_retry(format!(
                "SMTP Temp Failure/Greylisted? (4xx): {} {}",
                target_code, target_message
            ))
        }
        Severity::PermanentNegativeCompletion => {
            let rejection_phrases = [
                "unknown",
                "no such",
                "unavailable",
                "rejected",
                "doesn't exist",
                "disabled",
                "invalid address",
                "recipient not found",
                "user unknown",
                "mailbox unavailable",
            ];
            let message_lower = target_message.to_lowercase();

            let code_value = u16::from(target_code);

            if [550, 551, 553].contains(&code_value)
                || rejection_phrases.iter().any(|p| message_lower.contains(p))
            {
                SmtpVerificationResult::conclusive(
                    false,
                    format!(
                        "SMTP Rejected (User Likely Unknown): {} {}",
                        target_code, target_message
                    ),
                    false,
                )
            } else {
                SmtpVerificationResult::conclusive(
                    false,
                    format!(
                        "SMTP Rejected (Policy/Other 5xx): {} {}",
                        target_code, target_message
                    ),
                    false,
                )
            }
        }
    };

    smtp_conn
        .quit()
        .map_err(|e| {
            tracing::warn!(target: "smtp_task", "Error during SMTP QUIT: {}", e);
            AppError::Smtp(e)
        })
        .ok();

    Ok(final_result)
}

/// Helper function to interpret lettre::transport::smtp::Error into SmtpVerificationResult
fn handle_smtp_error(
    error: &lettre::transport::smtp::Error,
    server: &str,
) -> SmtpVerificationResult {
    let err_string = error.to_string();

    if err_string.contains("550")
        && (err_string.contains("does not exist")
            || err_string.contains("no such user")
            || err_string.contains("user unknown")
            || err_string.contains("recipient not found")
            || err_string.contains("NoSuchUser"))
    {
        return SmtpVerificationResult::conclusive(
            false,
            format!("SMTP Rejected (User Does Not Exist): {}", err_string),
            false,
        );
    }

    if err_string.contains("4")
        && (err_string.contains("temporary") || err_string.contains("transient"))
    {
        return SmtpVerificationResult::inconclusive_retry(format!(
            "SMTP Transient Error: {}",
            err_string
        ));
    }

    if err_string.contains("5") && err_string.contains("permanent") {
        return SmtpVerificationResult::inconclusive_no_retry(format!(
            "SMTP Permanent Error: {}",
            err_string
        ));
    }

    if err_string.contains("connection refused") {
        return SmtpVerificationResult::inconclusive_no_retry(format!(
            "Connection refused by {}",
            server
        ));
    }

    if err_string.contains("connection reset") {
        return SmtpVerificationResult::inconclusive_retry(format!(
            "Connection reset by {}",
            server
        ));
    }

    if err_string.contains("timed out") {
        return SmtpVerificationResult::inconclusive_retry(
            "SMTP connection/operation timed out".to_string(),
        );
    }

    if err_string.contains("TLS") || err_string.contains("tls") {
        tracing::warn!(target: "smtp_task", "SMTP TLS Error for {}: {:?}", server, error);
        return SmtpVerificationResult::inconclusive_retry(format!("SMTP TLS Error: {:?}", error));
    }

    tracing::error!(target: "smtp_task", "Unhandled SMTP Error ({}) : {}", server, error);
    SmtpVerificationResult::inconclusive_retry(format!("Unhandled SMTP Error: {}", error))
}

/// Verifies an email using SMTP with retries for inconclusive results.
///
/// # Arguments
/// * `email` - The email address to verify.
/// * `domain` - The domain part of the email address.
/// * `mail_server` - The hostname or IP address of the mail server.
///
/// # Returns
/// * `(Option<bool>, String)`: Tuple containing the verification status (Some(true), Some(false), or None)
///   and a final descriptive message.
pub(crate) async fn verify_email_smtp_with_retries(
    email: &str,
    domain: &str,
    mail_server: &str,
) -> (Option<bool>, String) {
    let mut last_result: Option<bool> = None;
    let mut last_message = "SMTP check did not run or complete".to_string();

    for attempt in 0..CONFIG.max_verification_attempts {
        tracing::info!(target: "smtp_task",
            "Attempt {}/{} SMTP check for {} via {}",
            attempt + 1,
            CONFIG.max_verification_attempts,
            email,
            mail_server
        );

        match verify_smtp_email(email, domain, mail_server).await {
            Ok(result) => {
                last_result = result.exists;
                last_message = result.message.clone();

                if result.exists.is_some() {
                    tracing::debug!(target: "smtp_task",
                        "SMTP check conclusive (Result: {:?}) on attempt {}.",
                        result.exists, attempt + 1
                    );
                    break;
                }

                if !result.should_retry {
                    tracing::warn!(target: "smtp_task",
                        "SMTP check failed with non-retriable status on attempt {}. Stopping. Msg: {}",
                         attempt + 1, result.message
                    );
                    break;
                }

                tracing::warn!(target: "smtp_task",
                    "SMTP check inconclusive on attempt {}. Message: {}",
                     attempt + 1, result.message
                );
            }
            Err(e) => {
                tracing::error!(target: "smtp_task",
                    "Error during SMTP verification attempt {}: {}", attempt + 1, e
                );
                last_message = format!("Internal error during SMTP check: {}", e);
                if attempt >= CONFIG.max_verification_attempts - 1 {
                    last_result = None;
                }
            }
        }

        if attempt < CONFIG.max_verification_attempts - 1 && last_result.is_none() {
            let sleep_duration = get_random_sleep_duration();
            tracing::debug!(target: "smtp_task", "Sleeping {:?} before next SMTP attempt.", sleep_duration);
            tokio::time::sleep(sleep_duration).await;
        }
    }

    tracing::info!(target: "smtp_task",
        "Final SMTP verification result for {}: Status={:?}, Msg='{}'",
        email, last_result, last_message
    );

    (last_result, last_message)
}

pub(crate) async fn test_smtp_connectivity() -> Result<()> {
    tracing::info!("Testing SMTP connectivity...");

    let socket_addr = match ("gmail-smtp-in.l.google.com", 25_u16)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
    {
        Some(addr) => addr,
        None => {
            return Err(AppError::Config(
                "Could not resolve gmail-smtp-in.l.google.com".to_string(),
            ));
        }
    };

    let helo_name = lettre::transport::smtp::extension::ClientId::Domain("localhost".to_string());

    let timeout = Duration::from_secs(5);

    match tokio::time::timeout(timeout, async {
        SmtpConnection::connect(socket_addr, Some(timeout), &helo_name, None, None)
    })
    .await
    {
        Ok(Ok(_)) => {
            tracing::info!("SMTP connectivity test successful");
            Ok(())
        }
        Ok(Err(e)) => Err(AppError::Smtp(e)),
        Err(_) => Err(AppError::SmtpInconclusive(
            "SMTP connection timed out - port 25 is likely blocked by your ISP or network provider"
                .to_string(),
        )),
    }
}
