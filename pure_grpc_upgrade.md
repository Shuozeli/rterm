# Upgrade `pure-grpc-rs` for WebTransport/gRPC Multiplexing

Currently, `rterm-relay` runs a custom `h3_body.rs` adapter in its codebase. This is a hack!
The `pure-grpc-rs` library is our own library, and it should natively support multiplexing standard gRPC calls alongside custom WebTransport sessions on the exact same `quinn::Endpoint`. 

## The Missing Capability in `pure-grpc-rs`

`pure-grpc-rs/grpc-server/src/h3_server.rs` currently implements an `H3Server` struct that calls `endpoint.accept()` and consumes raw QUIC connections. It assumes *every* connection is pure HTTP/3 and eagerly converts it to `http::Request<Body>` meant to be dispatched to a `Router`. 

If a user wants to accept a `CONNECT` method with `Protocol::WEB_TRANSPORT` (as `rterm-relay` does for the GUI terminal), they cannot use `H3Server::serve_endpoint` because it hides the `h3` connection loop and provides no callback or fallback for WebTransport streams.

## Required Implementation in `pure-grpc-rs`

The agent should modify the `pure-grpc-rs` git repository to securely expose HTTP/3 -> gRPC routing functionality for developers managing their own `h3_quinn` connections.

### 1. Export `serve_h3_request`
In `grpc_server`, expose a public method (e.g., `pub async fn serve_h3_request<S>(req, stream, service)`) or `pub async fn handle_request(resolver, svc)`. 
This function should encapsulate the `H3RecvBody` logic, splitting the `h3` bidi stream, wrapping the receive half as `grpc_core::body::Body`, polling the tower service, and encoding the gRPC trailers on the outgoing stream.

### 2. Cleanup `rterm-relay`
Once `pure-grpc-rs` has been patched and its Git ref updated in `rterm-relay/Cargo.toml`, the agent must revisit `rterm-relay`:
1. Delete `crates/rterm-relay/src/h3_body.rs`.
2. Remove `pub mod h3_body` from `crates/rterm-relay/src/lib.rs`.
3. In `crates/rterm-relay/src/main.rs`, replace the manual `h3_body` handler block inside the `h3_conn.accept` loop with the new `grpc_server::h3_server::handle_request(...)` function from the updated `pure-grpc-rs` library.
4. Ensure `cargo test -p rterm-relay` passes completely.

By doing this, `pure-grpc-rs` becomes a vastly more powerful framework capable of slotting neatly into custom WebTransport/UDP servers like `rterm-relay`.
