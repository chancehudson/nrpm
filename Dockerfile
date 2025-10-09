# Multi-stage build for smaller final image
FROM rust:1.87-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

COPY . ./

# Build the actual application
RUN cargo build --bin=onyx #--release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app
RUN mkdir package_data

# Copy binary from builder stage
#COPY --from=builder /app/target/release/onyx ./onyx
COPY --from=builder /app/target/debug/onyx ./onyx

# Expose port (adjust as needed)
EXPOSE 3000

ENV GIT_AUTHOR_NAME="onyx"
ENV GIT_AUTHOR_EMAIL="onyx@nrpm.io"
ENV GIT_COMMITTER_NAME="onyx"
ENV GIT_COMMITTER_EMAIL="onyx@nrpm.io"

# Run the server
CMD ["./onyx"]
