use crate::pty::PtySession;
use crate::screen_diff::{self, PrevScreen};
use grpc_codec_flatbuffers::FlatBuffersCodec;
use grpc_core::body::Body;
use grpc_core::{BoxFuture, Request, Response, Status, Streaming};
use grpc_server::{Grpc, NamedService, StreamingService};
use rterm_core::Terminal;
use rterm_proto::*;
use std::convert::Infallible;
use std::task::{Context, Poll};
use tokio_stream::Stream;
use tracing::{debug, info};

const DEFAULT_SHELL: &str = "/bin/bash";

#[derive(Clone, Default)]
pub struct TerminalServer {
    shell: String,
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

            let (cols, rows) = match input.message().await? {
                Some(ClientMsg::Resize(r)) => (r.cols, r.rows),
                Some(_) => return Err(Status::invalid_argument("first message must be Resize")),
                None => return Err(Status::invalid_argument("empty stream")),
            };

            info!("spawning PTY: shell={}, size={}x{}", shell, cols, rows);

            let pty = PtySession::spawn(&shell, cols, rows)
                .map_err(|e| Status::internal(format!("failed to spawn PTY: {e}")))?;

            let stdin_tx = pty.stdin_tx;
            let resize_tx = pty.resize_tx;

            tokio::spawn(async move {
                while let Ok(Some(msg)) = input.message().await {
                    match msg {
                        ClientMsg::KeyInput(k) => {
                            if stdin_tx.send(k.data).await.is_err() {
                                break;
                            }
                        }
                        ClientMsg::PasteInput(p) => {
                            if stdin_tx.send(p.text.into_bytes()).await.is_err() {
                                break;
                            }
                        }
                        ClientMsg::Resize(r) => {
                            if resize_tx.send((r.cols, r.rows)).await.is_err() {
                                break;
                            }
                        }
                        ClientMsg::MouseEvent(_) => {}
                    }
                }
                debug!("client input stream ended");
            });

            let mut terminal = Terminal::new(cols as usize, rows as usize);
            let mut prev = PrevScreen::new(cols as usize, rows as usize);
            let mut stdout_rx = pty.stdout_rx;

            let ss = screen_diff::snapshot(terminal.screen());
            prev.update_from_snapshot(&ss);
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            let _ = tx.send(Ok(ServerMsg::ScreenSnapshot(ss))).await;

            tokio::spawn(async move {
                while let Some(data) = stdout_rx.recv().await {
                    terminal.feed(&data);
                    if terminal.is_sync_mode() {
                        continue;
                    }
                    if let Some(update) = prev.diff(terminal.screen())
                        && tx.send(Ok(ServerMsg::ScreenUpdate(update))).await.is_err()
                    {
                        break;
                    }
                }
                let _ = tx.send(Ok(ServerMsg::Exit(Exit { code: 0 }))).await;
            });

            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
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
}
