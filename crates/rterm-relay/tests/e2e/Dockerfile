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

# Stage 2: Build rterm-relay and rterm-cli
FROM rust:latest AS rust-builder

WORKDIR /app
# Copy the full workspace
COPY . .

# Build both binaries in release mode
# Note: cargo handles the workspace automatically from the workspace root
RUN cargo build --release -p rterm-relay -p rterm-cli

# Stage 3: Runtime
# Match the runtime glibc closely to the builder toolchain environment so the
# release binaries produced by rust:latest remain loadable in the container.
FROM ubuntu:24.04

# Install basic runtime dependencies (ca-certificates for HTTPS/WSS, bash for the default PTY shell)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    bash \
    curl \
    python3 \
    vim \
    && rm -rf /var/lib/apt/lists/*

# Setup paths
WORKDIR /opt/rterm

# Copy WASM Dist
COPY --from=wasm-builder /app/crates/rterm-wasm/dist ./dist

# Copy Binaries
COPY --from=rust-builder /app/target/release/rterm-relay /usr/local/bin/rterm-relay
COPY --from=rust-builder /app/target/release/rterm-cli /usr/local/bin/rterm-cli

# Create a default configuration pointing to our internal Docker filesystem
# The config exposes WebTransport on 4433, H2 on 4434, and H3 on 4435.
RUN echo 'static_dir = "/opt/rterm/dist"\n\
\n\
[[listener]]\n\
protocol = "webtransport"\n\
port = 4433\n\
tls = true\n\
\n\
[[listener]]\n\
protocol = "grpc-h2"\n\
port = 4434\n\
tls = false\n\
\n\
[[listener]]\n\
protocol = "grpc-h3"\n\
port = 4435\n\
tls = true\n\
' > /etc/rterm.toml

EXPOSE 4433/udp
EXPOSE 4434/tcp
EXPOSE 4435/udp

ENTRYPOINT ["rterm-relay", "--config", "/etc/rterm.toml"]
