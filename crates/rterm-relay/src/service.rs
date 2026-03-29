use crate::pty::RealPtySpawner;
use crate::session;
use grpc_codec_flatbuffers::FlatBuffersCodec;
use grpc_core::body::Body;
use grpc_core::{BoxFuture, Request, Response, Status, Streaming};
use grpc_server::{Grpc, NamedService, StreamingService};
use rterm_proto::*;
use std::convert::Infallible;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tracing::debug;

const DEFAULT_SHELL: &str = "/bin/bash";

#[derive(Clone)]
pub struct TerminalServer {
    shell: String,
}

impl Default for TerminalServer {
    fn default() -> Self {
        Self {
            shell: DEFAULT_SHELL.to_string(),
        }
    }
}

impl TerminalServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shell(shell: impl Into<String>) -> Self {
        Self {
            shell: shell.into(),
        }
    }
}

impl NamedService for TerminalServer {
    const NAME: &'static str = "rterm.protocol.TerminalService";
}

type SessionResponseStream =
    std::pin::Pin<Box<dyn Stream<Item = Result<ServerMsg, Status>> + Send>>;

impl StreamingService<ClientMsg> for TerminalSvc {
    type Response = ServerMsg;
    type ResponseStream = SessionResponseStream;
    type Future = BoxFuture<Result<Response<Self::ResponseStream>, Status>>;

    fn call(&mut self, request: Request<Streaming<ClientMsg>>) -> Self::Future {
        let shell = self.0.clone();
        Box::pin(async move {
            let mut input = request.into_inner();

            // Bridge gRPC Streaming<ClientMsg> -> mpsc channel -> session.
            let (client_tx, mut client_rx) = mpsc::channel(64);
            let (server_tx, server_rx) = mpsc::channel(64);

            // Forward gRPC stream to channel.
            tokio::spawn(async move {
                while let Ok(Some(msg)) = input.message().await {
                    if client_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                debug!("gRPC client stream ended");
            });

            // Run session in background.
            let spawner = RealPtySpawner;
            tokio::spawn(async move {
                if let Err(e) =
                    session::run_session(&mut client_rx, &server_tx, &spawner, &shell).await
                {
                    debug!("session error: {}", e);
                }
            });

            // Convert channel to response stream.
            let stream = tokio_stream::wrappers::ReceiverStream::new(server_rx).map(Ok);
            Ok(Response::new(Box::pin(stream) as SessionResponseStream))
        })
    }
}

struct TerminalSvc(String);

impl tower_service::Service<http::Request<Body>> for TerminalServer {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Result<http::Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let shell = self.shell.clone();
        match req.uri().path() {
            "/rterm.protocol.TerminalService/Session" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<ServerMsg, ClientMsg>::default());
                Ok(grpc.streaming(TerminalSvc(shell), req).await)
            }),
            _ => Box::pin(async { Ok(Status::unimplemented("").into_http()) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_server_named_service() {
        assert_eq!(TerminalServer::NAME, "rterm.protocol.TerminalService");
    }

    #[test]
    fn terminal_server_is_clone() {
        let s = TerminalServer::new();
        let _s2 = s.clone();
    }

    #[test]
    fn terminal_server_default() {
        let s = TerminalServer::default();
        assert_eq!(s.shell, DEFAULT_SHELL);
    }

    #[test]
    fn terminal_server_with_shell() {
        let s = TerminalServer::with_shell("/bin/zsh");
        assert_eq!(s.shell, "/bin/zsh");
    }
}
