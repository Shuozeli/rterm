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
            let resp = http::Response::builder()
                .status(404)
                .body(())
                .expect("valid HTTP response");
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
        .expect("valid HTTP response");

    stream.send_response(resp).await?;
    stream.send_data(Bytes::from(data)).await?;
    stream.finish().await?;

    debug!("served: {} ({})", file_path.display(), content_type);
    Ok(())
}

pub fn resolve_path(uri_path: &str, static_dir: &Path) -> PathBuf {
    let clean = uri_path.trim_start_matches('/');
    if clean.is_empty() {
        return static_dir.join("index.html");
    }
    // Reject any path component that is ".." to prevent traversal.
    for component in Path::new(clean).components() {
        if matches!(component, std::path::Component::ParentDir) {
            return static_dir.join("index.html");
        }
    }
    static_dir.join(clean)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolve_path_root() {
        let dir = Path::new("/srv/static");
        assert_eq!(
            resolve_path("/", dir),
            PathBuf::from("/srv/static/index.html")
        );
    }

    #[test]
    fn resolve_path_empty() {
        let dir = Path::new("/srv/static");
        assert_eq!(
            resolve_path("", dir),
            PathBuf::from("/srv/static/index.html")
        );
    }

    #[test]
    fn resolve_path_file() {
        let dir = Path::new("/srv/static");
        assert_eq!(
            resolve_path("/app.js", dir),
            PathBuf::from("/srv/static/app.js")
        );
    }

    #[test]
    fn resolve_path_nested() {
        let dir = Path::new("/srv/static");
        assert_eq!(
            resolve_path("/assets/style.css", dir),
            PathBuf::from("/srv/static/assets/style.css")
        );
    }

    #[test]
    fn resolve_path_traversal_blocked() {
        let dir = Path::new("/srv/static");
        // Path traversal attempt should resolve to index.html.
        let result = resolve_path("/../../../etc/passwd", dir);
        assert_eq!(result, PathBuf::from("/srv/static/index.html"));
    }

    #[test]
    fn resolve_path_no_leading_slash() {
        let dir = Path::new("/srv/static");
        assert_eq!(
            resolve_path("file.wasm", dir),
            PathBuf::from("/srv/static/file.wasm")
        );
    }

    #[test]
    fn content_type_html() {
        assert_eq!(
            guess_content_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
    }

    #[test]
    fn content_type_js() {
        assert_eq!(
            guess_content_type(Path::new("app.js")),
            "application/javascript"
        );
    }

    #[test]
    fn content_type_wasm() {
        assert_eq!(
            guess_content_type(Path::new("module.wasm")),
            "application/wasm"
        );
    }

    #[test]
    fn content_type_css() {
        assert_eq!(guess_content_type(Path::new("style.css")), "text/css");
    }

    #[test]
    fn content_type_png() {
        assert_eq!(guess_content_type(Path::new("image.png")), "image/png");
    }

    #[test]
    fn content_type_ico() {
        assert_eq!(guess_content_type(Path::new("favicon.ico")), "image/x-icon");
    }

    #[test]
    fn content_type_unknown() {
        assert_eq!(
            guess_content_type(Path::new("data.bin")),
            "application/octet-stream"
        );
    }

    #[test]
    fn content_type_no_extension() {
        assert_eq!(
            guess_content_type(Path::new("Makefile")),
            "application/octet-stream"
        );
    }
}
