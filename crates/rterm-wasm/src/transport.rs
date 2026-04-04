/// WebTransport-based terminal transport for the browser.
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::WebTransport;

/// Sender half of a WebTransport connection.
pub struct WtSender {
    writer: web_sys::WritableStreamDefaultWriter,
}

/// Receiver half of a WebTransport connection.
pub struct WtReceiver {
    reader: web_sys::ReadableStreamDefaultReader,
}

/// Connect via WebTransport (QUIC/H3).
///
/// `url` should be like `wss://localhost:4433/wt`.
/// `cert_hash` is the SHA-256 hash of the server's certificate (for self-signed certs).
pub async fn connect(
    url: &str,
    cert_hash: Option<&[u8]>,
) -> Result<(WtSender, WtReceiver, WebTransport), String> {
    let opts = web_sys::WebTransportOptions::new();

    if let Some(hash) = cert_hash {
        let hash_obj = web_sys::WebTransportHash::new();
        hash_obj.set_algorithm("sha-256");
        let uint8 = js_sys::Uint8Array::from(hash);
        hash_obj.set_value(&uint8.buffer());
        opts.set_server_certificate_hashes(&[hash_obj]);
    }

    let transport = WebTransport::new_with_options(url, &opts)
        .map_err(|e| format!("WebTransport::new failed: {:?}", e))?;

    JsFuture::from(transport.ready())
        .await
        .map_err(|e| format!("WebTransport ready failed: {:?}", e))?;

    let bidi: web_sys::WebTransportBidirectionalStream =
        JsFuture::from(transport.create_bidirectional_stream())
            .await
            .map_err(|e| format!("createBidirectionalStream failed: {:?}", e))?
            .unchecked_into();

    let writable: web_sys::WritableStream = bidi.writable().unchecked_into();
    let readable: web_sys::ReadableStream = bidi.readable().unchecked_into();

    let writer = writable
        .get_writer()
        .map_err(|e| format!("getWriter failed: {:?}", e))?;
    let reader: web_sys::ReadableStreamDefaultReader =
        readable.get_reader().unchecked_into();

    Ok((WtSender { writer }, WtReceiver { reader }, transport))
}

impl WtSender {
    /// Send raw bytes to the server.
    pub async fn send(&self, data: &[u8]) -> Result<(), String> {
        let uint8 = js_sys::Uint8Array::from(data);
        JsFuture::from(self.writer.write_with_chunk(&uint8))
            .await
            .map_err(|e| format!("write failed: {:?}", e))?;
        Ok(())
    }
}

impl WtReceiver {
    /// Receive raw bytes from the server. Returns None if the stream is done.
    pub async fn recv(&self) -> Result<Option<Vec<u8>>, String> {
        let result = JsFuture::from(self.reader.read())
            .await
            .map_err(|e| format!("read failed: {:?}", e))?;

        let done = js_sys::Reflect::get(&result, &"done".into())
            .map_err(|e| format!("reflect done: {:?}", e))?
            .as_bool()
            .unwrap_or(true);

        if done {
            return Ok(None);
        }

        let value = js_sys::Reflect::get(&result, &"value".into())
            .map_err(|e| format!("reflect value: {:?}", e))?;

        let uint8: js_sys::Uint8Array = value.unchecked_into();
        Ok(Some(uint8.to_vec()))
    }
}
