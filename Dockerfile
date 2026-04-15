# ============================================
# Makima API - Multi-stage Docker Build
# ============================================
# Build stage: Compiles the Rust binary with musl target
# Runtime stage: Minimal Alpine Linux image with just the binary

# ============================================
# Stage 1: Builder
# ============================================
FROM rust:1.94-alpine AS builder

# Install build dependencies for musl target
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    postgresql-dev

# Set working directory
WORKDIR /app

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

# Build the application in release mode
# Use --release for optimized binary
RUN cargo build --release --bin api

# ============================================
# Stage 2: Runtime
# ============================================
FROM alpine:3.23

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc

# Create non-root user for security
RUN addgroup -g 1000 makima && \
    adduser -D -u 1000 -G makima makima

# Set working directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/api /app/makima

# Change ownership to non-root user
RUN chown -R makima:makima /app

# Switch to non-root user
USER makima

# Set environment defaults
ENV RUST_LOG=makima=info

# Expose the API port
EXPOSE 3000

# Run the application
CMD ["/app/makima"]
