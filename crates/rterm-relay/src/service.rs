use crate::pty::PtySession;
use grpc_codec_flatbuffers::FlatBuffersCodec;
use grpc_core::body::Body;
use grpc_core::{BoxFuture, Request, Response, Status, Streaming};
use grpc_server::{Grpc, NamedService, StreamingService};
use rterm_proto::{ClientMsg, DataOut, ServerMsg};
use std::convert::Infallible;
use std::task::{Context, Poll};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, info};

/// The shell command to spawn for each PTY session.
const DEFAULT_SHELL: &str = "/bin/bash";

/// gRPC service that handles the TerminalService.Session bidi stream.
#[derive(Clone)]
pub struct TerminalServer {
    shell: String,
}

impl Default for TerminalServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalServer {
    pub fn new() -> Self {
        Self {
            shell: DEFAULT_SHELL.to_string(),
        }
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

/// The response stream type: sends ServerMsg to the client.
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

            // Wait for the first message, which must be a Resize to set initial terminal size.
            let (initial_cols, initial_rows) = match input.message().await? {
                Some(ClientMsg::Resize(r)) => (r.cols, r.rows),
                Some(_) => {
                    return Err(Status::invalid_argument(
                        "first message must be Resize with initial terminal size",
                    ));
                }
                None => {
                    return Err(Status::invalid_argument("empty stream"));
                }
            };

            info!(
                "spawning PTY: shell={}, size={}x{}",
                shell, initial_cols, initial_rows
            );

            // Spawn PTY.
            let pty = PtySession::spawn(&shell, initial_cols, initial_rows)
                .map_err(|e| Status::internal(format!("failed to spawn PTY: {e}")))?;

            let stdin_tx = pty.stdin_tx;
            let resize_tx = pty.resize_tx;

            // Spawn task to forward client messages to PTY.
            tokio::spawn(async move {
                while let Ok(Some(msg)) = input.message().await {
                    match msg {
                        ClientMsg::DataIn(d) => {
                            if stdin_tx.send(d.payload).await.is_err() {
                                debug!("PTY stdin channel closed");
                                break;
                            }
                        }
                        ClientMsg::Resize(r) => {
                            if resize_tx.send((r.cols, r.rows)).await.is_err() {
                                debug!("PTY resize channel closed");
                                break;
                            }
                        }
                    }
                }
                debug!("client input stream ended");
            });

            // Create response stream from PTY stdout.
            let stdout_rx = pty.stdout_rx;
            let response_stream = tokio_stream::wrappers::ReceiverStream::new(stdout_rx)
                .map(|data| Ok(ServerMsg::DataOut(DataOut { payload: data })));

            Ok(Response::new(
                Box::pin(response_stream) as SessionResponseStream
            ))
        })
    }
}

/// Internal service dispatch wrapper.
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
}
