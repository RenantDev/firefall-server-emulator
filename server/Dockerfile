# === Build Stage ===
FROM rust:1-bookworm AS builder

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Build real app
COPY src/ src/
COPY migrations/ migrations/
RUN touch src/main.rs && cargo build --release

# === Runtime Stage ===
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    openssl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/firefall-server ./
COPY migrations/ ./migrations/

# Gerar certificado TLS auto-assinado com SAN para localhost E 127.0.0.1
RUN openssl req -x509 -newkey rsa:2048 \
    -keyout /app/key.pem -out /app/cert.pem \
    -days 3650 -nodes -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1'

EXPOSE 8443 25000

ENV RUST_LOG=firefall_server=info,tower_http=info

CMD ["./firefall-server"]
