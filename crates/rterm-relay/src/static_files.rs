/// Serve static files (WASM bundle) over HTTP/3.
use bytes::Bytes;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Serve a static file from the given directory.
pub async fn serve_file(
    stream: &mut h3::server::RequestStream<h3_quinn::BidiStream<bytes::Bytes>, bytes::Bytes>,
    uri_path: &str,
    static_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_path = resolve_path(uri_path, static_dir);

    let data = match tokio::fs::read(&file_path).await {
        Ok(data) => data,
        Err(_) => {
            debug!("404: {}", file_path.display());
            let resp = http::Response::builder().status(404).body(()).unwrap();
            stream.send_response(resp).await?;
            stream.send_data(Bytes::from("Not Found")).await?;
            stream.finish().await?;
            return Ok(());
        }
    };

    let content_type = guess_content_type(&file_path);

    let resp = http::Response::builder()
        .status(200)
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
        .body(())
        .unwrap();

    stream.send_response(resp).await?;
    stream.send_data(Bytes::from(data)).await?;
    stream.finish().await?;

    debug!("served: {} ({})", file_path.display(), content_type);
    Ok(())
}

pub fn resolve_path(uri_path: &str, static_dir: &Path) -> PathBuf {
    let clean = uri_path.trim_start_matches('/');
    let path = if clean.is_empty() {
        static_dir.join("index.html")
    } else {
        static_dir.join(clean)
    };
    // Prevent path traversal.
    if path.starts_with(static_dir) {
        path
    } else {
        static_dir.join("index.html")
    }
}

pub fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
