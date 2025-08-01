# Build stage
FROM rust:1.75-slim AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN echo "=== Build context contents ===" && find . -type f -name "*.toml" -o -name "*.rs" | head -20

# Copy workspace files from src/rust directory
COPY Cargo.toml Cargo.lock ./
COPY . .

# COPY . .
RUN ls -la

WORKDIR /app/src/rust

# Build the fcb_api binary
# RUN cargo build --release -p fcb_api

# # Runtime stage
# FROM debian:bookworm-slim

# # Install runtime dependencies
# RUN apt-get update && apt-get install -y \
#     ca-certificates \
#     libssl3 \
#     && rm -rf /var/lib/apt/lists/*

# # Copy the binary from builder
# COPY --from=builder /app/target/release/fcb_api /usr/local/bin/fcb_api

# # Set environment variables for Cloud Run
# ENV HOST=0.0.0.0
# ENV PORT=8080

# # Expose the port
# EXPOSE 8080

# # Run the binary
# CMD ["fcb_api"]