use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use grpc_core::{decode_grpc_frame, encode_grpc_frame};
use reqwest::Client;
use rterm_proto::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const SERVER_URL: &str = "https://127.0.0.1:14434/rterm.protocol.TerminalService";

/// A wrapper that starts docker-compose and automatically tears it down when dropped.
struct DockerHarness;

impl DockerHarness {
    pub fn new() -> Self {
        println!("==> Starting Docker container...");
        let build_status = Command::new("docker")
            .args([
                "compose",
                "-f",
                "tests/e2e/docker-compose.yml",
                "up",
                "--build",
                "-d",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .expect("Failed to execute docker compose");

        assert!(
            build_status.success(),
            "Failed to build & start docker container"
        );
        Self
    }
}

impl Drop for DockerHarness {
    fn drop(&mut self) {
        println!("==> Tearing down Docker container...");
        let _ = Command::new("docker")
            .args([
                "compose",
                "-f",
                "tests/e2e/docker-compose.yml",
                "down",
                "-v",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
    }
}

async fn wait_for_server(client: &Client) {
    let url = format!("{}/ListActiveSessions", SERVER_URL);
    let mut attempts = 0;

    // We only need an empty struct for list sessions pinging
    let grpc_payload = encode_grpc_frame(&UnaryListSessionsRequest {}.encode_flatbuffer());

    println!("==> Waiting for HTTP/2 port 14434 to become healthy...");

    while attempts < 30 {
        match client
            .post(&url)
            .header("content-type", "application/grpc")
            .body(grpc_payload.clone())
            .send()
            .await
        {
            Ok(_) => {
                println!("==> Server is healthy!");
                // Wait another 0.5 seconds just to assure everything inside the container boot process finished.
                sleep(Duration::from_millis(500)).await;
                return;
            }
            Err(_) => {
                attempts += 1;
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
    panic!("Docker container never became healthy!");
}

async fn send_grpc_h2c(client: &Client, method: &str, payload: Vec<u8>) -> Vec<u8> {
    let url = format!("{}/{}", SERVER_URL, method);

    let grpc_payload = encode_grpc_frame(&payload);

    let resp = client
        .post(&url)
        .header("content-type", "application/grpc")
        .body(grpc_payload)
        .send()
        .await
        .expect("HTTP req failed completely");

    assert!(
        resp.status().is_success(),
        "HTTP req returned failed status: {:?}",
        resp.status()
    );

    let bytes = resp.bytes().await.unwrap();
    assert!(
        bytes.len() >= 5,
        "Response too short for gRPC framing, received {} bytes",
        bytes.len()
    );
    decode_grpc_frame(&bytes).to_vec()
}

async fn unary_call<Req, Resp>(client: &Client, method: &str, request: Req) -> Resp
where
    Req: FlatBufferGrpcMessage,
    Resp: FlatBufferGrpcMessage,
{
    let bytes = send_grpc_h2c(client, method, request.encode_flatbuffer()).await;
    Resp::decode_flatbuffer(&bytes).unwrap_or_else(|e| panic!("decode {} failed: {}", method, e))
}

async fn list_sessions(client: &Client) -> UnaryListSessionsResponse {
    unary_call(client, "ListActiveSessions", UnaryListSessionsRequest {}).await
}

async fn create_session(client: &Client, session_name: &str, cols: u16, rows: u16) {
    let resp: CreateSessionResponse = unary_call(
        client,
        "CreateSession",
        CreateSessionRequest {
            session_name: session_name.into(),
            shell: String::new(),
            cols,
            rows,
        },
    )
    .await;
    assert!(
        resp.success,
        "create failed for {}: {}",
        session_name, resp.error
    );
}

async fn kill_session(client: &Client, session_name: &str) {
    let resp: KillSessionResponse = unary_call(
        client,
        "KillSession",
        KillSessionRequest {
            session_name: session_name.into(),
        },
    )
    .await;
    assert!(
        resp.success,
        "kill failed for {}: {}",
        session_name, resp.error
    );
}

async fn type_text(client: &Client, session_name: &str, text: &str) {
    let resp: TypeResponse = unary_call(
        client,
        "TypeAction",
        TypeRequest {
            session_name: session_name.into(),
            text: text.into(),
        },
    )
    .await;
    assert!(
        resp.success,
        "type failed for {}: {}",
        session_name, resp.error
    );
}

async fn press_keys(client: &Client, session_name: &str, keys: &[&str]) {
    let resp: PressKeysResponse = unary_call(
        client,
        "PressKeys",
        PressKeysRequest {
            session_name: session_name.into(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        },
    )
    .await;
    assert!(
        resp.success,
        "press failed for {}: {}",
        session_name, resp.error
    );
}

async fn run_command(
    client: &Client,
    session_name: &str,
    command: &str,
    timeout_ms: u64,
) -> RunCommandResponse {
    unary_call(
        client,
        "RunCommand",
        RunCommandRequest {
            session_name: session_name.into(),
            command: command.into(),
            timeout_ms,
        },
    )
    .await
}

async fn wait_for_text_rpc(
    client: &Client,
    session_name: &str,
    pattern: &str,
    timeout_ms: u64,
) -> WaitForTextResponse {
    unary_call(
        client,
        "WaitForText",
        WaitForTextRequest {
            session_name: session_name.into(),
            pattern: pattern.into(),
            timeout_ms,
        },
    )
    .await
}

async fn resize_session(client: &Client, session_name: &str, cols: u16, rows: u16) {
    let resp: ResizeSessionResponse = unary_call(
        client,
        "ResizeSession",
        ResizeSessionRequest {
            session_name: session_name.into(),
            cols,
            rows,
        },
    )
    .await;
    assert!(
        resp.success,
        "resize failed for {}: {}",
        session_name, resp.error
    );
}

async fn get_snapshot(client: &Client, session_name: &str) -> GetSnapshotResponse {
    unary_call(
        client,
        "GetSnapshot",
        GetSnapshotRequest {
            session_name: session_name.into(),
        },
    )
    .await
}

#[tokio::test]
#[ignore = "requires Docker; run with `cargo test -- --ignored`"]
async fn test_docker_e2e_automation_scenarios() {
    let _harness = DockerHarness::new();

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    wait_for_server(&client).await;

    let list_resp = list_sessions(&client).await;
    assert_eq!(
        list_resp.sessions.len(),
        0,
        "expected a fresh container runtime"
    );

    // Scenario A: simple command output.
    create_session(&client, "e2e-simple", 80, 24).await;
    let run = run_command(&client, "e2e-simple", "echo hello-world", 10_000).await;
    assert!(!run.timed_out, "simple run timed out: {:?}", run.output);
    assert!(
        run.output.contains("hello-world"),
        "missing hello-world: {:?}",
        run.output
    );
    kill_session(&client, "e2e-simple").await;

    // Scenario B: multi-command state persists within one running relay process.
    create_session(&client, "e2e-state", 80, 24).await;
    let export = run_command(&client, "e2e-state", "export FOO=bar", 10_000).await;
    assert!(!export.timed_out, "export timed out: {:?}", export.output);
    let echo = run_command(&client, "e2e-state", "echo $FOO", 10_000).await;
    assert!(!echo.timed_out, "echo $FOO timed out: {:?}", echo.output);
    assert!(
        echo.output.contains("bar"),
        "missing persisted env var: {:?}",
        echo.output
    );
    kill_session(&client, "e2e-state").await;

    // Scenario C: vim open, edit, save, quit.
    create_session(&client, "e2e-vim", 80, 24).await;
    type_text(&client, "e2e-vim", "vim /tmp/rterm-test.txt\n").await;
    let vim_open = wait_for_text_rpc(&client, "e2e-vim", "~", 10_000).await;
    assert!(
        vim_open.found,
        "vim did not open: {:?}",
        vim_open.plain_text
    );
    type_text(&client, "e2e-vim", "i").await;
    let insert = wait_for_text_rpc(&client, "e2e-vim", "INSERT", 5_000).await;
    assert!(
        insert.found,
        "vim did not enter insert mode: {:?}",
        insert.plain_text
    );
    type_text(&client, "e2e-vim", "hello from rterm automation").await;
    press_keys(&client, "e2e-vim", &["Escape"]).await;
    type_text(&client, "e2e-vim", ":wq").await;
    press_keys(&client, "e2e-vim", &["Enter"]).await;
    sleep(Duration::from_millis(500)).await;
    let cat = run_command(&client, "e2e-vim", "cat /tmp/rterm-test.txt", 10_000).await;
    assert!(!cat.timed_out, "cat after vim timed out: {:?}", cat.output);
    assert!(
        cat.output.contains("hello from rterm automation"),
        "vim save failed: {:?}",
        cat.output
    );
    kill_session(&client, "e2e-vim").await;

    // Scenario D: vim navigation and search.
    create_session(&client, "e2e-vimnav", 80, 24).await;
    let write_nav = run_command(
        &client,
        "e2e-vimnav",
        "printf 'line1\nline2\nline3\n' > /tmp/nav.txt",
        10_000,
    )
    .await;
    assert!(
        !write_nav.timed_out,
        "nav fixture write timed out: {:?}",
        write_nav.output
    );
    type_text(&client, "e2e-vimnav", "vim /tmp/nav.txt\n").await;
    let line1 = wait_for_text_rpc(&client, "e2e-vimnav", "line1", 10_000).await;
    assert!(
        line1.found,
        "vim nav did not open file: {:?}",
        line1.plain_text
    );
    type_text(&client, "e2e-vimnav", "/line3").await;
    press_keys(&client, "e2e-vimnav", &["Enter"]).await;
    sleep(Duration::from_millis(500)).await;
    let nav_snapshot = get_snapshot(&client, "e2e-vimnav").await;
    assert!(
        nav_snapshot.plain_text.contains("line3"),
        "line3 not visible after search: {:?}",
        nav_snapshot.plain_text
    );
    assert_eq!(
        nav_snapshot.snapshot.cursor.row, 2,
        "expected vim cursor on row 2 after /line3 search"
    );
    press_keys(&client, "e2e-vimnav", &["Escape"]).await;
    type_text(&client, "e2e-vimnav", ":q!").await;
    press_keys(&client, "e2e-vimnav", &["Enter"]).await;
    kill_session(&client, "e2e-vimnav").await;

    // Scenario E: Python REPL.
    create_session(&client, "e2e-py", 80, 24).await;
    type_text(&client, "e2e-py", "python3\n").await;
    let py_prompt = wait_for_text_rpc(&client, "e2e-py", ">>>", 10_000).await;
    assert!(
        py_prompt.found,
        "python prompt not found: {:?}",
        py_prompt.plain_text
    );
    type_text(&client, "e2e-py", "2 + 2\n").await;
    let py_result = wait_for_text_rpc(&client, "e2e-py", "4", 5_000).await;
    assert!(
        py_result.found,
        "python result not found: {:?}",
        py_result.plain_text
    );
    let py_snapshot = get_snapshot(&client, "e2e-py").await;
    assert!(
        py_snapshot.plain_text.contains(">>>"),
        "python prompt did not return"
    );
    type_text(&client, "e2e-py", "exit()\n").await;
    sleep(Duration::from_millis(500)).await;
    let post_py = run_command(&client, "e2e-py", "echo shell-back", 10_000).await;
    assert!(
        !post_py.timed_out,
        "shell did not recover after python: {:?}",
        post_py.output
    );
    assert!(
        post_py.output.contains("shell-back"),
        "missing shell-back: {:?}",
        post_py.output
    );
    kill_session(&client, "e2e-py").await;

    // Scenario F: Ctrl+C interrupts a running process.
    create_session(&client, "e2e-sigint", 80, 24).await;
    type_text(&client, "e2e-sigint", "sleep 60\n").await;
    sleep(Duration::from_millis(500)).await;
    press_keys(&client, "e2e-sigint", &["Ctrl+C"]).await;
    sleep(Duration::from_millis(500)).await;
    let post_sigint = run_command(&client, "e2e-sigint", "echo interrupted", 10_000).await;
    assert!(
        !post_sigint.timed_out,
        "shell did not recover after Ctrl+C: {:?}",
        post_sigint.output
    );
    assert!(
        post_sigint.output.contains("interrupted"),
        "missing interrupted marker: {:?}",
        post_sigint.output
    );
    let sigint_snapshot = get_snapshot(&client, "e2e-sigint").await;
    assert!(
        sigint_snapshot.plain_text.contains("sleep 60"),
        "original sleep command line not visible after Ctrl+C: {:?}",
        sigint_snapshot.plain_text
    );
    kill_session(&client, "e2e-sigint").await;

    // Scenario G: resize mid-session.
    create_session(&client, "e2e-resize", 80, 24).await;
    let hi = run_command(&client, "e2e-resize", "echo hi", 10_000).await;
    assert!(
        !hi.timed_out,
        "resize setup command timed out: {:?}",
        hi.output
    );
    resize_session(&client, "e2e-resize", 120, 40).await;
    let resized = get_snapshot(&client, "e2e-resize").await;
    assert_eq!(resized.snapshot.cols, 120);
    assert_eq!(resized.snapshot.num_rows, 40);
    kill_session(&client, "e2e-resize").await;

    // Scenario H: WaitForText timeout path.
    create_session(&client, "e2e-timeout", 80, 24).await;
    let start = Instant::now();
    let timeout = wait_for_text_rpc(&client, "e2e-timeout", "XYZZY", 300).await;
    let elapsed = start.elapsed();
    assert!(
        !timeout.found,
        "unexpectedly found XYZZY: {:?}",
        timeout.plain_text
    );
    assert!(
        elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(2),
        "unexpected timeout latency: {:?}",
        elapsed
    );
    kill_session(&client, "e2e-timeout").await;

    // Verify cleanup returned us to zero live sessions.
    let list_resp = list_sessions(&client).await;
    assert_eq!(
        list_resp.sessions.len(),
        0,
        "expected no live sessions after cleanup"
    );
}
