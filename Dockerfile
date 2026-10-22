# Build stage
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code and config
COPY src ./src
COPY config.toml ./

# Build optimized release binary
RUN cargo build --release --bin axiom-bot

# Production runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary and configuration
COPY --from=builder /app/target/release/axiom-bot /usr/local/bin/axiom-bot
COPY --from=builder /app/config.toml /app/config.toml

# Set default logging level
ENV RUST_LOG=info

# Run bot as 24/7 daemon
CMD ["/usr/local/bin/axiom-bot"]
