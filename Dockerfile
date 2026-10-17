# Multi-stage Dockerfile for cal-share
FROM rust:1.88-alpine AS builder

# Install build dependencies for musl
RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /app

# Pre-fetch and cache dependencies
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# Copy real source code and build
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

# Final minimal runtime image
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

# Create unprivileged user
RUN addgroup -S appgroup && adduser -S appuser -G appgroup

# Create storage directory for calendars
RUN mkdir -p /app/data/calendars && chown -R appuser:appgroup /app/data

# Copy compiled binary from builder
COPY --from=builder /app/target/release/cal-share /app/cal-share

USER appuser

ENV PORT=8080
ENV HOST=0.0.0.0
ENV CALENDAR_STORAGE_DIR=/app/data/calendars

EXPOSE 8080

CMD ["/app/cal-share"]
