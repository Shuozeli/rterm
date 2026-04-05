use clap::{Parser, Subcommand};
use grpc_client::{Channel, Endpoint, Grpc};
use grpc_codec_flatbuffers::{FlatBufferGrpcMessage, FlatBuffersCodec};
use grpc_core::Status;
use http::uri::PathAndQuery;
use rterm_proto::*;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "rterm-cli")]
#[command(about = "Playwright-style headless automation client for rterm", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// RPC endpoint
    #[arg(short, long, default_value = "https://localhost:4433")]
    endpoint: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // ── Session lifecycle ───────────────────────────────────────────────────
    /// List active terminal sessions
    List,

    /// Create a named session (explicit shell/size control)
    Create {
        session: String,
        /// Shell to launch (empty → server default /bin/bash)
        #[arg(short, long, default_value = "")]
        shell: String,
        /// Terminal width in columns
        #[arg(short, long, default_value_t = 220)]
        cols: u16,
        /// Terminal height in rows
        #[arg(short, long, default_value_t = 50)]
        rows: u16,
    },

    /// Destroy a named session (kill the PTY)
    Kill { session: String },

    /// Resize a session's terminal
    Resize {
        session: String,
        #[arg(short, long)]
        cols: u16,
        #[arg(short, long)]
        rows: u16,
    },

    // ── Input ───────────────────────────────────────────────────────────────
    /// Send UTF-8 text to a session (no newline appended)
    Type { session: String, text: String },

    /// Launch an interactive program (sends "<command>\n", returns immediately)
    ///
    /// Use this for TUIs (vim, less, htop) and REPLs (python3, node).
    /// Follow with `wait` to detect when the program has started.
    Exec { session: String, command: String },

    /// Send named key presses (server resolves to correct PTY bytes per VT mode)
    ///
    /// Key names: Enter, Escape, Tab, Backspace, Delete,
    /// Up/Down/Left/Right, Home, End, PageUp, PageDown,
    /// Ctrl+C, Ctrl+D, Ctrl+Z, Ctrl+L, Ctrl+A, Ctrl+E, Ctrl+U, Ctrl+W,
    /// F1–F12.
    ///
    /// Example: rterm-cli press myses Escape Enter
    Press {
        session: String,
        /// One or more key names
        #[arg(required = true)]
        keys: Vec<String>,
    },

    /// Send raw PTY bytes using Rust escape syntax (\x03, \x1b[A, etc.)
    SendKeys {
        session: String,
        /// Escape string, e.g. "\\x1b[A"
        keys: String,
    },

    // ── Output ──────────────────────────────────────────────────────────────
    /// Print the current screen as plain text
    GetText { session: String },

    /// Print the current screen as a Rust debug struct
    Snapshot { session: String },

    /// Print the current screen as JSON
    SnapshotJson { session: String },

    /// Wait until a substring appears on screen (server polls every 100ms)
    Wait {
        session: String,
        pattern: String,
        #[arg(short, long, default_value_t = 5000)]
        timeout_ms: u64,
    },

    /// Assert a substring is visible right now (exit 1 if not)
    Assert { session: String, pattern: String },

    // ── Higher-level automation ──────────────────────────────────────────────
    /// Run a shell command and return only the new output (blocks until done)
    Run {
        session: String,
        command: String,
        #[arg(short, long, default_value_t = 10000)]
        timeout_ms: u64,
    },
}

fn relay_cert_path() -> PathBuf {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    config_dir.join("rterm").join("cert.pem")
}

async fn make_grpc(endpoint: &str) -> Result<Grpc<Channel>, String> {
    let uri: http::Uri = endpoint
        .parse()
        .map_err(|e| format!("invalid endpoint URI: {e}"))?;

    let channel = if uri.scheme_str() == Some("https") {
        let cert_path = relay_cert_path();
        let ca_pem = std::fs::read(&cert_path).map_err(|e| {
            format!(
                "could not read relay CA cert from {}: {} \
                 (start the relay first, or use http:// for plaintext)",
                cert_path.display(),
                e
            )
        })?;
        Endpoint::new(uri.clone())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .tls_with_ca(ca_pem)
            .connect()
            .await
            .map_err(|e| format!("TLS connection failed: {e}"))?
    } else {
        Endpoint::new(uri.clone())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .connect()
            .await
            .map_err(|e| format!("connection failed: {e}"))?
    };

    Ok(Grpc::with_origin(channel, uri))
}

async fn call<Req, Resp>(
    grpc: &mut Grpc<Channel>,
    method: &str,
    request: Req,
) -> Result<Resp, Status>
where
    Req: FlatBufferGrpcMessage,
    Resp: FlatBufferGrpcMessage,
{
    let path: PathAndQuery = format!("/rterm.protocol.TerminalService/{}", method)
        .parse()
        .expect("valid path");
    grpc.unary(request, path, FlatBuffersCodec::<Req, Resp>::new())
        .await
        .map(|r| r.into_inner())
}

/// Parse Rust-style escape strings into raw bytes.
/// `"\x03"` → `[0x03]`, `"\x1b[A"` → `[0x1b, 0x5b, 0x41]`.
fn parse_escape_str(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'x' if i + 3 < bytes.len() => {
                    if let Ok(b) = u8::from_str_radix(&s[i + 2..i + 4], 16) {
                        out.push(b);
                        i += 4;
                        continue;
                    }
                }
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                    continue;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                    continue;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn snapshot_to_json(snap: &ScreenSnapshotData, plain_text: &str) -> String {
    let cursor = &snap.cursor;
    let mut cells = Vec::new();
    for row in &snap.rows {
        for cell in &row.cells {
            cells.push(format!(
                r#"{{"row":{},"col":{},"ch":{:?},"fg":{},"bg":{},"flags":{}}}"#,
                row.row, row.col_start, cell.ch, cell.fg, cell.bg, cell.flags
            ));
        }
    }
    format!(
        r#"{{"cols":{},"rows":{},"cursor":{{"row":{},"col":{},"visible":{}}},"alt_screen_active":{},"application_cursor_keys":{},"plain_text":{},"cells":[{}]}}"#,
        snap.cols,
        snap.num_rows,
        cursor.row,
        cursor.col,
        cursor.visible,
        snap.alt_screen_active,
        snap.application_cursor_keys,
        serde_json_str(plain_text),
        cells.join(",")
    )
}

/// Minimal JSON string escaping (no serde dependency).
fn serde_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{}", msg);
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();

    let mut grpc = make_grpc(&cli.endpoint)
        .await
        .unwrap_or_else(|e| die(format!("Error: {}", e)));

    match cli.command {
        Commands::List => {
            let resp = call::<UnaryListSessionsRequest, UnaryListSessionsResponse>(
                &mut grpc,
                "ListActiveSessions",
                UnaryListSessionsRequest {},
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));

            println!(
                "{0: <20} | {1: <10} | {2: <10}",
                "SESSION", "COLSxROWS", "IDLE (s)"
            );
            println!("{:-<20}-+-{:-<10}-+-{:-<10}", "", "", "");
            for s in resp.sessions {
                println!(
                    "{0: <20} | {1:<4}x{2:<4} | {3:<10}",
                    s.name, s.cols, s.rows, s.last_activity
                );
            }
        }

        Commands::Create {
            session,
            shell,
            cols,
            rows,
        } => {
            let resp = call::<CreateSessionRequest, CreateSessionResponse>(
                &mut grpc,
                "CreateSession",
                CreateSessionRequest {
                    session_name: session,
                    shell,
                    cols,
                    rows,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
            println!("Session created.");
        }

        Commands::Kill { session } => {
            let resp = call::<KillSessionRequest, KillSessionResponse>(
                &mut grpc,
                "KillSession",
                KillSessionRequest {
                    session_name: session,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
            println!("Session killed.");
        }

        Commands::Resize {
            session,
            cols,
            rows,
        } => {
            let resp = call::<ResizeSessionRequest, ResizeSessionResponse>(
                &mut grpc,
                "ResizeSession",
                ResizeSessionRequest {
                    session_name: session,
                    cols,
                    rows,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
            println!("Resized to {}x{}.", cols, rows);
        }

        Commands::Type { session, text } => {
            let resp = call::<TypeRequest, TypeResponse>(
                &mut grpc,
                "TypeAction",
                TypeRequest {
                    session_name: session,
                    text,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
        }

        Commands::Exec { session, command } => {
            // Exec = type the command followed by Enter. Returns immediately.
            let text = format!("{}\n", command.trim_end_matches('\n'));
            let resp = call::<TypeRequest, TypeResponse>(
                &mut grpc,
                "TypeAction",
                TypeRequest {
                    session_name: session,
                    text,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
        }

        Commands::Press { session, keys } => {
            let resp = call::<PressKeysRequest, PressKeysResponse>(
                &mut grpc,
                "PressKeys",
                PressKeysRequest {
                    session_name: session,
                    keys,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
        }

        Commands::SendKeys { session, keys } => {
            let raw = parse_escape_str(&keys);
            let resp = call::<SendKeysRequest, SendKeysResponse>(
                &mut grpc,
                "SendKeys",
                SendKeysRequest {
                    session_name: session,
                    keys: raw,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.success {
                die(format!("Failed: {}", resp.error));
            }
        }

        Commands::GetText { session } => {
            let resp = call::<GetSnapshotRequest, GetSnapshotResponse>(
                &mut grpc,
                "GetSnapshot",
                GetSnapshotRequest {
                    session_name: session,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            print!("{}", resp.plain_text);
        }

        Commands::Snapshot { session } => {
            let resp = call::<GetSnapshotRequest, GetSnapshotResponse>(
                &mut grpc,
                "GetSnapshot",
                GetSnapshotRequest {
                    session_name: session,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            println!("{:#?}", resp.snapshot);
        }

        Commands::SnapshotJson { session } => {
            let resp = call::<GetSnapshotRequest, GetSnapshotResponse>(
                &mut grpc,
                "GetSnapshot",
                GetSnapshotRequest {
                    session_name: session,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            println!("{}", snapshot_to_json(&resp.snapshot, &resp.plain_text));
        }

        Commands::Wait {
            session,
            pattern,
            timeout_ms,
        } => {
            let resp = call::<WaitForTextRequest, WaitForTextResponse>(
                &mut grpc,
                "WaitForText",
                WaitForTextRequest {
                    session_name: session,
                    pattern: pattern.clone(),
                    timeout_ms,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if resp.found {
                print!("{}", resp.plain_text);
            } else {
                die(format!(
                    "Timeout: pattern {:?} not found within {}ms\n{}",
                    pattern, timeout_ms, resp.plain_text
                ));
            }
        }

        Commands::Assert { session, pattern } => {
            // Point-in-time check: timeout_ms=0 → server checks once and returns.
            let resp = call::<WaitForTextRequest, WaitForTextResponse>(
                &mut grpc,
                "WaitForText",
                WaitForTextRequest {
                    session_name: session,
                    pattern: pattern.clone(),
                    timeout_ms: 0,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            if !resp.found {
                die(format!(
                    "Assertion failed: {:?} not on screen\n{}",
                    pattern, resp.plain_text
                ));
            }
        }

        Commands::Run {
            session,
            command,
            timeout_ms,
        } => {
            let resp = call::<RunCommandRequest, RunCommandResponse>(
                &mut grpc,
                "RunCommand",
                RunCommandRequest {
                    session_name: session,
                    command,
                    timeout_ms,
                },
            )
            .await
            .unwrap_or_else(|e| die(format!("RPC error: {}", e)));
            print!("{}", resp.output);
            if resp.timed_out {
                die("[timed out]");
            }
        }
    }
}
