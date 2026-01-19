# Build stage: compile the Rust application
# Using debian-based rust image for better compatibility with newer Rust features
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests first for better caching
COPY Cargo.toml ./

# Create a dummy src/main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src

# Build the real application
# Touch main.rs to force rebuild with new source
RUN touch src/main.rs && cargo build --release

# Runtime stage: minimal image with just the binary
FROM debian:bookworm-slim

# Add ca-certificates and minimal runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /build/target/release/threshold-node .

# Run the application
CMD ["./threshold-node"]
