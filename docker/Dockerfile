# Stage 1: Build rterm-wasm
FROM rust:latest AS wasm-builder

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk

WORKDIR /app
COPY . .

# Build the WASM GUI
WORKDIR /app/crates/rterm-wasm
ENV RUSTFLAGS="--cfg web_sys_unstable_apis"
RUN trunk build --release

# Stage 2: Build rterm-relay
FROM rust:latest AS relay-builder

WORKDIR /app
COPY . .

# Build relay binary in release mode
RUN cargo build --release -p rterm-relay

# Stage 3: Runtime
FROM ubuntu:24.04

# Install basic runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    bash \
    curl \
    vim \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/rterm

# Copy WASM dist
COPY --from=wasm-builder /app/crates/rterm-wasm/dist ./dist

# Copy relay binary
COPY --from=relay-builder /app/target/release/rterm-relay /usr/local/bin/rterm-relay

# Copy default config
COPY docker/rterm.toml /etc/rterm.toml

# Health check port
ENV RTERM_HEALTH_PORT=9090

EXPOSE 8080 8081 9090

ENTRYPOINT ["rterm-relay", "--config", "/etc/rterm.toml", "--insecure", "--ws-insecure"]
