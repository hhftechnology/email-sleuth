# Email Sleuth

A tool to find and verify email addresses associated with contacts and company websites.

## Features

- Generate potential email addresses based on name patterns
- Scrape websites for email addresses
- Verify email addresses using SMTP
- Command-line interface for batch processing
- API server with web UI for interactive use
- Docker support for easy deployment

## Installation

### Using Cargo

```bash
cargo install --git https://github.com/tokenizer-decode/email-sleuth
```

### Using Docker

```bash
# Clone the repository
git clone https://github.com/tokenizer-decode/email-sleuth
cd email-sleuth

# Build and run with Docker Compose
docker-compose up -d
```

## Usage

### Command Line

Process a JSON file containing contacts:

```bash
email-sleuth process --input contacts.json --output results.json --workers 5
```

Start the API server:

```bash
email-sleuth serve --port 8080
```

### API Endpoints

The API server provides the following endpoints:

- `GET /health` - Health check endpoint
- `GET /ui` - Web UI for interactive use
- `POST /verify` - Verify a single contact
- `POST /batch` - Process multiple contacts

#### Single Contact Verification

```bash
curl -X POST http://localhost:8080/verify \
  -H "Content-Type: application/json" \
  -d '{
    "first_name": "John",
    "last_name": "Doe",
    "domain": "example.com"
  }'
```

#### Batch Processing

```bash
curl -X POST http://localhost:8080/batch \
  -H "Content-Type: application/json" \
  -d '{
    "contacts": [
      {
        "first_name": "John",
        "last_name": "Doe",
        "domain": "example.com"
      },
      {
        "first_name": "Jane",
        "last_name": "Smith",
        "domain": "anothercompany.com"
      }
    ]
  }'
```

### Web UI

The web UI is available at http://localhost:8080/ui when the API server is running.

## Input Format

The input JSON file should contain an array of contact objects with the following fields:

```json
[
  {
    "first_name": "John",
    "last_name": "Doe",
    "domain": "example.com"
  },
  {
    "first_name": "Jane",
    "last_name": "Smith",
    "domain": "anothercompany.com"
  }
]
```

## Docker Deployment

The included Dockerfile and docker-compose.yml make it easy to deploy Email Sleuth:

1. Build the Docker image:
   ```bash
   docker build -t email-sleuth .
   ```

2. Run the container:
   ```bash
   docker run -p 8080:8080 email-sleuth serve --port 8080
   ```

Or simply use Docker Compose:
```bash
docker-compose up -d
```

## Configuration

Email Sleuth can be configured using the `email-sleuth.toml` file. See the example configuration file for available options.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
