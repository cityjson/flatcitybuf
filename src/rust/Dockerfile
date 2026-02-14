# Build stage
FROM rust:1.86-slim AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app


# Copy workspace files from src/rust directory
COPY src/rust/Cargo.toml ./
COPY src/rust ./

# Remove workspace members that aren't needed for the API and require
# extra system dependencies (C++ compiler for fcb_cpp, Python headers for fcb_py)
RUN sed -i 's/members = \[.*\]/members = ["cli", "fcb_core", "wasm", "fcb_api"]/' Cargo.toml

RUN cargo build --release -p fcb_api

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/fcb_api /usr/local/bin/fcb_api

# Set environment variables for Cloud Run
ENV HOST=0.0.0.0
ENV PORT=8080

# Expose the port
EXPOSE 8080

# Run the binary
CMD ["fcb_api"]