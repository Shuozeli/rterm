pub mod session;

use grpc_codec_flatbuffers::FlatBuffersCodec;
use grpc_core::body::Body;
use grpc_core::{BoxFuture, Request, Response, Status, Streaming};
use grpc_server::{Grpc, NamedService, ServerStreamingService, StreamingService, UnaryService};
use rterm_proto::*;
use rterm_session::SessionManager;
use rterm_session::resolve_key;
use rterm_session::screen_diff;
use rterm_transport::{PtySpawner, RealPtySpawner};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::{Stream, StreamExt};
use tracing::debug;

const DEFAULT_SHELL: &str = "/bin/bash";

static RUN_CMD_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct TerminalServer {
    shell: String,
    session_mgr: Arc<SessionManager>,
    spawner: Arc<dyn PtySpawner>,
}

impl TerminalServer {
    pub fn new(session_mgr: Arc<SessionManager>) -> Self {
        Self {
            shell: DEFAULT_SHELL.to_string(),
            session_mgr,
            spawner: Arc::new(RealPtySpawner),
        }
    }

    pub fn with_shell(shell: impl Into<String>, session_mgr: Arc<SessionManager>) -> Self {
        Self {
            shell: shell.into(),
            session_mgr,
            spawner: Arc::new(RealPtySpawner),
        }
    }

    pub fn with_spawner(
        shell: impl Into<String>,
        session_mgr: Arc<SessionManager>,
        spawner: Arc<dyn PtySpawner>,
    ) -> Self {
        Self {
            shell: shell.into(),
            session_mgr,
            spawner,
        }
    }
}

impl NamedService for TerminalServer {
    const NAME: &'static str = "rterm.protocol.TerminalService";
}

type SessionResponseStream =
    std::pin::Pin<Box<dyn Stream<Item = Result<ServerMsg, Status>> + Send>>;

struct TerminalSvc(String, Arc<dyn PtySpawner>);

impl StreamingService<ClientMsg> for TerminalSvc {
    type Response = ServerMsg;
    type ResponseStream = SessionResponseStream;
    type Future = BoxFuture<Result<Response<Self::ResponseStream>, Status>>;

    fn call(&mut self, request: Request<Streaming<ClientMsg>>) -> Self::Future {
        let shell = self.0.clone();
        let spawner = Arc::clone(&self.1);
        Box::pin(async move {
            let mut input = request.into_inner();

            let (client_tx, client_rx) = mpsc::channel(64);
            let (server_tx, server_rx) = mpsc::channel(64);

            tokio::spawn(async move {
                while let Ok(Some(msg)) = input.message().await {
                    if client_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                debug!("gRPC client stream ended");
            });

            tokio::spawn(async move {
                if let Err(e) =
                    session::run_session(client_rx, &server_tx, spawner.as_ref(), &shell).await
                {
                    debug!("session error: {}", e);
                }
            });

            let stream = tokio_stream::wrappers::ReceiverStream::new(server_rx).map(Ok);
            Ok(Response::new(Box::pin(stream) as SessionResponseStream))
        })
    }
}

struct GetSnapshotSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<GetSnapshotRequest> for GetSnapshotSvc {
    type Response = GetSnapshotResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<GetSnapshotRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req_msg = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req_msg.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => return Err(Status::internal(format!("failed to create session: {}", e))),
            };

            let lock = session.lock().await;
            let plain_text = lock.plain_text();
            let mut snap = screen_diff::snapshot(lock.terminal.screen());
            snap.mouse_tracking_mode = lock.terminal.modes.mouse_tracking_mode;
            snap.alt_screen_active = lock.terminal.is_alt_screen_active();
            snap.application_cursor_keys = lock.terminal.modes.application_cursor_keys;

            let resp = GetSnapshotResponse {
                snapshot: snap,
                plain_text,
            };
            Ok(Response::new(resp))
        })
    }
}

struct TypeActionSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<TypeRequest> for TypeActionSvc {
    type Response = TypeResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<TypeRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req_msg = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req_msg.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => return Err(Status::internal(format!("failed to create session: {}", e))),
            };

            let stdin_tx = {
                let lock = session.lock().await;
                lock.pty_stdin_tx.clone()
            };
            let resp = match stdin_tx.send(req_msg.text.into_bytes()).await {
                Ok(()) => TypeResponse {
                    success: true,
                    error: String::new(),
                },
                Err(e) => TypeResponse {
                    success: false,
                    error: e.to_string(),
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct ListSessionsSvc(Arc<SessionManager>);
impl UnaryService<UnaryListSessionsRequest> for ListSessionsSvc {
    type Response = UnaryListSessionsResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, _request: Request<UnaryListSessionsRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        Box::pin(async move {
            let active = session_mgr.list_sessions().await;
            let resp = UnaryListSessionsResponse { sessions: active };
            Ok(Response::new(resp))
        })
    }
}

struct CreateSessionSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<CreateSessionRequest> for CreateSessionSvc {
    type Response = CreateSessionResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<CreateSessionRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req = request.into_inner();
        Box::pin(async move {
            let shell = if req.shell.is_empty() {
                DEFAULT_SHELL.to_string()
            } else {
                req.shell.clone()
            };
            let result = session_mgr
                .get_or_create_with_shell(
                    &req.session_name,
                    &shell,
                    req.cols,
                    req.rows,
                    spawner.as_ref(),
                )
                .await;
            let resp = match result {
                Ok(_) => CreateSessionResponse {
                    success: true,
                    error: String::new(),
                },
                Err(e) => CreateSessionResponse {
                    success: false,
                    error: e,
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct KillSessionSvc(Arc<SessionManager>);
impl UnaryService<KillSessionRequest> for KillSessionSvc {
    type Response = KillSessionResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<KillSessionRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let req = request.into_inner();
        Box::pin(async move {
            let resp = match session_mgr.destroy(&req.session_name).await {
                Ok(()) => KillSessionResponse {
                    success: true,
                    error: String::new(),
                },
                Err(e) => KillSessionResponse {
                    success: false,
                    error: e,
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct ResizeSessionSvc(Arc<SessionManager>);
impl UnaryService<ResizeSessionRequest> for ResizeSessionSvc {
    type Response = ResizeSessionResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<ResizeSessionRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let req = request.into_inner();
        Box::pin(async move {
            let resp = match session_mgr.get(&req.session_name).await {
                Some(session) => {
                    session.lock().await.resize(req.cols, req.rows);
                    ResizeSessionResponse {
                        success: true,
                        error: String::new(),
                    }
                }
                None => ResizeSessionResponse {
                    success: false,
                    error: format!("session '{}' not found", req.session_name),
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct SendKeysSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<SendKeysRequest> for SendKeysSvc {
    type Response = SendKeysResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<SendKeysRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    return Ok(Response::new(SendKeysResponse {
                        success: false,
                        error: format!("failed to get session: {}", e),
                    }));
                }
            };

            let stdin_tx = {
                let lock = session.lock().await;
                lock.pty_stdin_tx.clone()
            };
            let resp = match stdin_tx.send(req.keys).await {
                Ok(()) => SendKeysResponse {
                    success: true,
                    error: String::new(),
                },
                Err(e) => SendKeysResponse {
                    success: false,
                    error: e.to_string(),
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct WaitForTextSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<WaitForTextRequest> for WaitForTextSvc {
    type Response = WaitForTextResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<WaitForTextRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    return Ok(Response::new(WaitForTextResponse {
                        found: false,
                        plain_text: format!("error: {}", e),
                    }));
                }
            };

            let deadline = Instant::now() + Duration::from_millis(req.timeout_ms);
            let pattern = req.pattern;

            loop {
                let text = {
                    let lock = session.lock().await;
                    lock.plain_text()
                };
                if text.contains(&pattern) {
                    return Ok(Response::new(WaitForTextResponse {
                        found: true,
                        plain_text: text,
                    }));
                }
                if Instant::now() >= deadline {
                    return Ok(Response::new(WaitForTextResponse {
                        found: false,
                        plain_text: text,
                    }));
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
    }
}

struct PressKeysSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<PressKeysRequest> for PressKeysSvc {
    type Response = PressKeysResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<PressKeysRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    return Ok(Response::new(PressKeysResponse {
                        success: false,
                        error: format!("failed to get session: {}", e),
                    }));
                }
            };

            // Read application_cursor_keys and stdin_tx while holding the lock.
            let (app_cursor, stdin_tx) = {
                let lock = session.lock().await;
                (
                    lock.terminal.modes.application_cursor_keys,
                    lock.pty_stdin_tx.clone(),
                )
            };

            // Resolve all key names to bytes before sending.
            let mut payload: Vec<u8> = Vec::new();
            for key_name in &req.keys {
                match resolve_key(key_name, app_cursor) {
                    Some(bytes) => payload.extend_from_slice(&bytes),
                    None => {
                        return Ok(Response::new(PressKeysResponse {
                            success: false,
                            error: format!("unknown key name: {:?}", key_name),
                        }));
                    }
                }
            }

            let resp = match stdin_tx.send(payload).await {
                Ok(()) => PressKeysResponse {
                    success: true,
                    error: String::new(),
                },
                Err(e) => PressKeysResponse {
                    success: false,
                    error: e.to_string(),
                },
            };
            Ok(Response::new(resp))
        })
    }
}

struct RunCommandSvc(Arc<SessionManager>, Arc<dyn PtySpawner>);
impl UnaryService<RunCommandRequest> for RunCommandSvc {
    type Response = RunCommandResponse;
    type Future = BoxFuture<Result<Response<Self::Response>, Status>>;

    fn call(&mut self, request: Request<RunCommandRequest>) -> Self::Future {
        let session_mgr = Arc::clone(&self.0);
        let spawner = Arc::clone(&self.1);
        let req = request.into_inner();
        Box::pin(async move {
            let session = match session_mgr
                .get_or_create(&req.session_name, 80, 24, spawner.as_ref())
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    return Ok(Response::new(RunCommandResponse {
                        output: format!("error: {}", e),
                        timed_out: false,
                    }));
                }
            };

            // Use a unique sentinel so we can reliably detect command completion
            // without relying on shell prompt detection.
            // AtomicU64 counter guarantees uniqueness across concurrent calls.
            let sentinel = format!(
                "RTERM_DONE_{:x}",
                RUN_CMD_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let wrapped = format!(
                "{}; echo \"{}\"\n",
                req.command.trim_end_matches('\n'),
                sentinel
            );

            // Snapshot the screen before sending so we can strip pre-existing content.
            let text_before = {
                let lock = session.lock().await;
                lock.plain_text()
            };
            let lines_before: std::collections::HashSet<&str> = text_before.lines().collect();

            let stdin_tx = {
                let lock = session.lock().await;
                lock.pty_stdin_tx.clone()
            };
            if stdin_tx.send(wrapped.into_bytes()).await.is_err() {
                return Ok(Response::new(RunCommandResponse {
                    output: "error: PTY closed".into(),
                    timed_out: false,
                }));
            }

            let timeout_ms = if req.timeout_ms == 0 {
                10_000
            } else {
                req.timeout_ms
            };
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);

            loop {
                let text = {
                    let lock = session.lock().await;
                    lock.plain_text()
                };
                if text.contains(&sentinel) {
                    // Return only lines that are new since the command was sent,
                    // excluding the sentinel itself and the echoed command line.
                    let command_prefix = req.command.trim();
                    let output: String = text
                        .lines()
                        .filter(|l| {
                            let trimmed = l.trim();
                            !trimmed.is_empty()
                                && !trimmed.contains(&sentinel)
                                && !trimmed.contains(command_prefix)
                                && !lines_before.contains(*l)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    return Ok(Response::new(RunCommandResponse {
                        output,
                        timed_out: false,
                    }));
                }
                if Instant::now() >= deadline {
                    return Ok(Response::new(RunCommandResponse {
                        output: text,
                        timed_out: true,
                    }));
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
    }
}

struct ExecSvc(Arc<dyn PtySpawner>);

impl ServerStreamingService<ExecRequest> for ExecSvc {
    type Response = ExecResponse;
    type ResponseStream =
        std::pin::Pin<Box<dyn Stream<Item = Result<ExecResponse, Status>> + Send>>;
    type Future = BoxFuture<Result<Response<Self::ResponseStream>, Status>>;

    fn call(&mut self, request: Request<ExecRequest>) -> Self::Future {
        let spawner = Arc::clone(&self.0);
        let req = request.into_inner();
        Box::pin(async move {
            let mut handle = spawner
                .spawn_exec(&req.command, &req.cwd, 80, 24)
                .map_err(|e| Status::internal(format!("spawn failed: {}", e)))?;

            let timeout_ms = if req.timeout_ms == 0 {
                30_000
            } else {
                req.timeout_ms
            };
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);

            let stream = async_stream::stream! {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline.into()) => {
                            // Timeout reached
                            yield Ok(ExecResponse {
                                stdout: vec![],
                                stderr: vec![],
                                exit_code: -1,
                                timed_out: true,
                            });
                            break;
                        }

                        chunk = handle.stdout_rx.recv() => {
                            match chunk {
                                Some(data) => {
                                    yield Ok(ExecResponse {
                                        stdout: data,
                                        stderr: vec![],
                                        exit_code: -1,
                                        timed_out: false,
                                    });
                                }
                                None => {
                                    // No more stdout, wait for exit code
                                    break;
                                }
                            }
                        }

                        code = &mut handle.exit_code_rx => {
                            match code {
                                Ok(exit_code) => {
                                    yield Ok(ExecResponse {
                                        stdout: vec![],
                                        stderr: vec![],
                                        exit_code,
                                        timed_out: false,
                                    });
                                }
                                Err(_) => {
                                    yield Ok(ExecResponse {
                                        stdout: vec![],
                                        stderr: vec![],
                                        exit_code: -1,
                                        timed_out: false,
                                    });
                                }
                            }
                            break;
                        }
                    }
                }
            };

            Ok(Response::new(Box::pin(stream) as Self::ResponseStream))
        })
    }
}

impl tower_service::Service<http::Request<Body>> for TerminalServer {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Result<http::Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let shell = self.shell.clone();
        let session_mgr = Arc::clone(&self.session_mgr);
        let spawner = Arc::clone(&self.spawner);

        match req.uri().path() {
            "/rterm.protocol.TerminalService/Session" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<ServerMsg, ClientMsg>::default());
                Ok(grpc.streaming(TerminalSvc(shell, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/GetSnapshot" => Box::pin(async move {
                let mut grpc = Grpc::new(
                    FlatBuffersCodec::<GetSnapshotResponse, GetSnapshotRequest>::default(),
                );
                Ok(grpc.unary(GetSnapshotSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/TypeAction" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<TypeResponse, TypeRequest>::default());
                Ok(grpc.unary(TypeActionSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/ListActiveSessions" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<
                    UnaryListSessionsResponse,
                    UnaryListSessionsRequest,
                >::default());
                Ok(grpc.unary(ListSessionsSvc(session_mgr), req).await)
            }),
            "/rterm.protocol.TerminalService/CreateSession" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<
                    CreateSessionResponse,
                    CreateSessionRequest,
                >::default());
                Ok(grpc
                    .unary(CreateSessionSvc(session_mgr, spawner), req)
                    .await)
            }),
            "/rterm.protocol.TerminalService/KillSession" => Box::pin(async move {
                let mut grpc = Grpc::new(
                    FlatBuffersCodec::<KillSessionResponse, KillSessionRequest>::default(),
                );
                Ok(grpc.unary(KillSessionSvc(session_mgr), req).await)
            }),
            "/rterm.protocol.TerminalService/ResizeSession" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<
                    ResizeSessionResponse,
                    ResizeSessionRequest,
                >::default());
                Ok(grpc.unary(ResizeSessionSvc(session_mgr), req).await)
            }),
            "/rterm.protocol.TerminalService/SendKeys" => Box::pin(async move {
                let mut grpc =
                    Grpc::new(FlatBuffersCodec::<SendKeysResponse, SendKeysRequest>::default());
                Ok(grpc.unary(SendKeysSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/WaitForText" => Box::pin(async move {
                let mut grpc = Grpc::new(
                    FlatBuffersCodec::<WaitForTextResponse, WaitForTextRequest>::default(),
                );
                Ok(grpc.unary(WaitForTextSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/PressKeys" => Box::pin(async move {
                let mut grpc =
                    Grpc::new(FlatBuffersCodec::<PressKeysResponse, PressKeysRequest>::default());
                Ok(grpc.unary(PressKeysSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/RunCommand" => Box::pin(async move {
                let mut grpc =
                    Grpc::new(FlatBuffersCodec::<RunCommandResponse, RunCommandRequest>::default());
                Ok(grpc.unary(RunCommandSvc(session_mgr, spawner), req).await)
            }),
            "/rterm.protocol.TerminalService/Exec" => Box::pin(async move {
                let mut grpc = Grpc::new(FlatBuffersCodec::<ExecResponse, ExecRequest>::default());
                Ok(grpc.server_streaming(ExecSvc(spawner), req).await)
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
        let smgr = Arc::new(SessionManager::new(DEFAULT_SHELL));
        let s = TerminalServer::new(smgr);
        let _s2 = s.clone();
    }

    #[test]
    fn terminal_server_with_shell() {
        let smgr = Arc::new(SessionManager::new("/bin/zsh"));
        let s = TerminalServer::with_shell("/bin/zsh", smgr);
        assert_eq!(s.shell, "/bin/zsh");
    }

    #[test]
    fn resolve_key_normal_cursor() {
        assert_eq!(resolve_key("Up", false).unwrap(), b"\x1b[A");
        assert_eq!(resolve_key("Down", false).unwrap(), b"\x1b[B");
        assert_eq!(resolve_key("Right", false).unwrap(), b"\x1b[C");
        assert_eq!(resolve_key("Left", false).unwrap(), b"\x1b[D");
    }

    #[test]
    fn resolve_key_application_cursor() {
        assert_eq!(resolve_key("Up", true).unwrap(), b"\x1bOA");
        assert_eq!(resolve_key("Down", true).unwrap(), b"\x1bOB");
        assert_eq!(resolve_key("Right", true).unwrap(), b"\x1bOC");
        assert_eq!(resolve_key("Left", true).unwrap(), b"\x1bOD");
    }

    #[test]
    fn resolve_key_aliases() {
        assert_eq!(resolve_key("ArrowUp", false).unwrap(), b"\x1b[A");
        assert_eq!(resolve_key("Escape", false).unwrap(), b"\x1b");
        assert_eq!(resolve_key("Esc", false).unwrap(), b"\x1b");
        assert_eq!(resolve_key("Ctrl+C", false).unwrap(), b"\x03");
        assert_eq!(resolve_key("C-c", false).unwrap(), b"\x03");
    }

    #[test]
    fn resolve_key_unknown_returns_none() {
        assert!(resolve_key("Bogus", false).is_none());
        assert!(resolve_key("ctrl+x", false).is_none()); // case-sensitive
    }

    #[test]
    fn resolve_key_special() {
        assert_eq!(resolve_key("Enter", false).unwrap(), b"\r");
        assert_eq!(resolve_key("Tab", false).unwrap(), b"\t");
        assert_eq!(resolve_key("Backspace", false).unwrap(), b"\x7f");
        assert_eq!(resolve_key("Delete", false).unwrap(), b"\x1b[3~");
        assert_eq!(resolve_key("PageUp", false).unwrap(), b"\x1b[5~");
        assert_eq!(resolve_key("PageDown", false).unwrap(), b"\x1b[6~");
        assert_eq!(resolve_key("F1", false).unwrap(), b"\x1bOP");
        assert_eq!(resolve_key("F12", false).unwrap(), b"\x1b[24~");
    }
}
