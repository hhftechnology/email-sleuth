FROM rust:1.76-slim as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src/ ./src/
COPY email-sleuth.toml ./

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/email-sleuth /app/email-sleuth
COPY --from=builder /app/email-sleuth.toml /app/email-sleuth.toml

# Create a directory for data
RUN mkdir -p /app/data

# Expose the API port
EXPOSE 8080

# Set the entrypoint
ENTRYPOINT ["/app/email-sleuth"]
