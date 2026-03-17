# MultiLink Dockerfile
# Multi-stage build for MultiLink core library
#
# Usage:
#   docker build -t multilink-core .
#   docker run -v $(pwd)/config:/app/config -p 8080:8080 multilink-core

# ============================================================================
# Stage 1: Builder - compile Rust dependencies and core
# ============================================================================
FROM rust:1.75-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy only Cargo files for dependency caching
COPY core/Cargo.toml core/Cargo.lock ./core/
COPY gui/rust/chat_controller/Cargo.toml gui/rust/chat_controller/
WORKDIR /app/core
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo build --release
RUN rm -rf src

# Copy source and build
WORKDIR /app
COPY core/src ./core/src
WORKDIR /app/core
RUN cargo build --release

# ============================================================================
# Stage 2: Runtime - minimal runtime image
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libcurl4 \
    libssl3 \
    libgcc-s1 \
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/core/target/release/multilink-core /usr/local/bin/
COPY config/default.toml /app/config/

# Create non-root user for security
RUN useradd -m -u 1000 multilink && \
    chown -R multilink:multilink /app
USER multilink

# Default configuration - can be overridden via environment
ENV OLLAMA_HOST=http://host.docker.internal:11434
ENV MULTILINK_CONFIG_DIR=/app/config

EXPOSE 8080

CMD ["multilink-core", "--config", "/app/config/default.toml"]
