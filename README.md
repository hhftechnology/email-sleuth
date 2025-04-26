# Email Sleuth

A Rust application to discover and verify professional email addresses based on contact names and company websites. This tool helps you find valid email addresses for business contacts when you have their name and company domain.

## Features

-   **Pattern Generation**: Creates common email patterns based on first and last names.
-   **Website Scraping**: Crawls company websites for email addresses.
-   **SMTP Verification**: Validates email existence via direct mail server communication.
-   **Domain Intelligence**: Uses DNS (MX records) to find mail servers.
-   **Concurrent Processing**: Handles multiple contacts simultaneously.
-   **Ranking & Scoring**: Ranks possible email addresses by confidence.
-   **Detailed JSON Output**: Provides comprehensive results.
-   **CLI Mode**: Process single contacts directly from the command line.
-   **Configuration File**: Customize behavior via a TOML file.

## Prerequisites

-   **Operating System**: Linux, macOS, or Windows.
-   **CPU Architecture**: Pre-compiled binaries provided for `x86_64` (Intel/AMD 64-bit) and `aarch64` (ARM 64-bit, e.g., Apple Silicon, Raspberry Pi 4+).
-   **Outbound SMTP Access (Port 25)**: **Crucial** for email verification. Many ISPs/networks block this. See [SMTP Requirements](#smtp-requirements) below.

## Installation

Choose the method that best suits your system:

### 1. Installer Script (Linux & macOS - Recommended)

This script automatically detects your OS/architecture, downloads the latest release, and installs `email-sleuth` to a standard location (`$HOME/.local/bin` by default, or `/usr/local/bin` if run with `sudo`).

```bash
# Install latest version
curl -fsSL https://raw.githubusercontent.com/tokenizer-decode/email-sleuth/main/install.sh | bash

# Install a specific version
# curl -fsSL https://raw.githubusercontent.com/tokenizer-decode/email-sleuth/main/install.sh | bash -s -- --version vX.Y.Z
```

After installation, ensure the installation directory (`$HOME/.local/bin` or `/usr/local/bin`) is in your `PATH`. The script will provide instructions if it's not. You can then run `email-sleuth --version` to verify the installation.

### 2. Manual Binary Download (All Platforms)

If you prefer not to use the script, or are on Windows:

1.  Go to the [**Releases page**](https://github.com/tokenizer-decode/email-sleuth/releases).
2.  Find the latest release and locate the correct archive under "Assets" for your Operating System and Architecture.
3.  Download and extract the archive.
4.  Move the extracted `email-sleuth` (or `email-sleuth.exe` on Windows) executable to a directory in your system's `PATH`.
5.  **(Linux/macOS only)** Make the binary executable: `chmod +x /path/to/email-sleuth`.
6.  Verify by opening a **new** terminal and running: `email-sleuth --version`.

### 3. Build from Source (Developers)

Requires the Rust toolchain (>= 1.70 recommended).

```bash
# 1. Install Rust: https://www.rust-lang.org/tools/install
# 2. Clone the repository
git clone https://github.com/tokenizer-decode/email-sleuth.git
cd email-sleuth

# 3. Build the optimized release binary
cargo build --release

# 4. The executable is at target/release/email-sleuth (or .exe)
#    Copy it to your PATH (see step 4 of manual install)
```

## Usage

`email-sleuth` can operate in two main modes: finding an email for a single contact via command-line arguments, or processing a batch of contacts from a JSON file.

### 1. Single Contact Mode (CLI)

Provide the name and domain directly. Use `--stdout true` to print results to the console.

```bash
# Find email and print to console
email-sleuth --name "John Doe" --domain example.com --stdout true

# Find email and save detailed JSON results to a file
email-sleuth --name "Jane Smith" --domain company.com --output jane_smith_result.json
```

### 2. Batch Processing Mode (File I/O)

Process multiple contacts defined in an input JSON file and save results to an output JSON file.

```bash
# Use default input.json and output results.json
email-sleuth

# Specify input and output files
email-sleuth --input contacts_list.json --output found_emails.json

# Specify files and increase concurrency
email-sleuth -i contacts_list.json -o found_emails.json --concurrency 4
```

### Command-line Options

View all available options and their descriptions:

```bash
email-sleuth --help
```

Enable verbose logging using the `RUST_LOG` environment variable:

```bash
# Set log level (e.g., debug, trace)
export RUST_LOG=debug

# Run the command
email-sleuth --input input.json
# Windows: set RUST_LOG=debug (CMD) or $env:RUST_LOG="debug" (PowerShell)
```

### Input File Format (`input.json`) for Batch Mode

A JSON array of objects. Each object needs name fields (`first_name` and `last_name`) and a `domain`.

```json
[
  {
    "first_name": "John",
    "last_name": "Smith",
    "domain": "example.com"
  },
  {
    "first_name": "Jane",
    "last_name": "Doe",
    "domain": "acme.com"
  }
]
```
*(See `examples/example-contacts.json` for a more detailed example)*

### Output Format (`results.json`)

The tool produces a detailed JSON output for each contact processed. In CLI mode with `--stdout true`, a simplified summary is printed. When outputting to a file, the full structure is saved.

```json
// Example structure when saving to a file (results may vary)
[
  {
    "contact_input": { /* Original input contact data */ },
    "email": "john.smith@example.com", // Best guess found (or null)
    "confidence_score": 8,             // Confidence (0-10) for 'email'
    "found_emails": [                  // All plausible candidates found
      {
        "email": "john.smith@example.com",
        "confidence": 8,
        "source": "pattern", // "pattern" or "scraped"
        "is_generic": false,
        "verification_status": true, // SMTP result: true (exists), false (doesn't), null (inconclusive/skipped)
        "verification_message": "SMTP Verification OK: 250 2.1.5 Ok"
      }
      // ... other candidates
    ],
    "methods_used": ["pattern_generation", "smtp_verification"], // Could include "website_scraping"
    "verification_log": { /* Detailed SMTP/DNS check logs */ },
    "scraping_error": null, // Populated if scraping attempted and failed
    "email_finding_skipped": false, // True if input was invalid
    "email_finding_error": null   // Unexpected processing errors
  },
  // ... results for other contacts
]
```

## Configuration

`email-sleuth` uses a layered configuration system:

1.  **Command-line Arguments**: Highest priority. Overrides all other settings. (e.g., `--concurrency 8`)
2.  **Configuration File (TOML)**: Third priority. Loaded automatically if found.
3.  **Default Values**: Lowest priority. Used if no other setting is provided.

### Configuration File

You can customize default behavior by creating a TOML configuration file. `email-sleuth` automatically looks for this file in the following locations (in order):

1.  Path specified by `--config-file <path>` argument.
2.  `./email-sleuth.toml` (in the current directory)
3.  `./config.toml` (in the current directory)
4.  `~/.config/email-sleuth.toml` (user's config directory - Linux/macOS)

An example configuration file with all available options can be found here:
[**email-sleuth.toml**](https://github.com/tokenizer-decode/email-sleuth/blob/main/email-sleuth.toml)

This allows you to set defaults for timeouts, concurrency, DNS servers, sender email, scraping behavior, etc., without specifying them on the command line every time.

## How it Works

1. **Input Validation**: Checks for name and domain.
2. **Pattern Generation**: Creates likely email formats.
3. **Website Scraping**: Crawls website for public emails.
4. **Candidate Ranking**: Filters generics, sorts by likelihood.
5. **SMTP Verification**:
   * Finds mail servers via DNS MX lookup.
   * Connects to server (port 25) for promising candidates.
   * Uses SMTP commands (`HELO`, `MAIL FROM`, `RCPT TO`) to check if the server accepts the address.
6. **Scoring & Selection**: Assigns confidence based on source and verification; picks the best result.

## SMTP Requirements

**Email verification accuracy depends heavily on outbound SMTP (port 25) connectivity.**

- **Why?**: The tool directly queries the recipient's mail server on port 25 to confirm if an address exists.
- **Problem**: Most home ISPs and many corporate networks **block** outgoing port 25 to prevent spam.
- **Impact**: If blocked, SMTP verification fails or times out, resulting in lower confidence scores and `null` verification status.
- **Test**: The application attempts a basic SMTP connectivity test on startup and logs a warning if it fails.

**Solutions if Port 25 is Blocked**:
1. **Cloud Server (Recommended)**: Run on AWS EC2, Google Cloud, DigitalOcean, etc. (most allow port 25).
2. **VPN**: Use a VPN service known *not* to block outbound port 25 (research required).

**Without port 25 access, the tool generates patterns and scrapes websites but cannot reliably verify them.**

## Limitations

- **SMTP Blocking**: The most significant limitation (see above).
- **Catch-all Domains**: Servers accepting all emails defeat verification.
- **Greylisting**: Temporary rejections can lead to inconclusive results (`null`).
- **Scraping Limits**: Won't find emails behind logins or complex JavaScript.

## License

MIT License

## Author

Kerim Buyukakyuz - ([tokenizer-decode](https://github.com/tokenizer-decode))

## Contributing

Contributions, issues, and feature requests welcome via [GitHub Issues](https://github.com/tokenizer-decode/email-sleuth/issues) and Pull Requests.