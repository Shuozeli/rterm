/// Connection management: WebTransport connect, send/recv loops, reconnection with backoff.
/// Supports runtime transport selection:
/// - WebTransport (default): connects via WebTransport on port 4433, path /wt/{session}
/// - WebSocket: connects via WebSocket on port 4435, path /ws/{session}
/// Transport is selected via URL query parameter `?transport=ws` or `?transport=wt`
use crate::app::Shared;
use crate::messages::{decode_server_msg, encode_resize, ServerMsg};
use crate::protocol::{encode_message, RecvBuffer};
use crate::session;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;

mod ws_conn {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    async fn sleep_ms(ms: i32) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
                .unwrap();
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }

    pub struct WsSender {
        ws: web_sys::WebSocket,
    }

    pub struct WsReceiver {
        queue: Rc<RefCell<VecDeque<Vec<u8>>>>,
    }

    pub async fn connect_websocket(
        url: &str,
    ) -> Result<(WsSender, WsReceiver), String> {
        let ws = web_sys::WebSocket::new(url)
            .map_err(|e| format!("WebSocket::new failed: {:?}", e))?;

        let queue: Rc<RefCell<VecDeque<Vec<u8>>>> = Rc::new(RefCell::new(VecDeque::new()));
        let queue_clone = Rc::clone(&queue);

        // Set up message event handler to push to queue.
        let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            if let Ok(data) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let uint8 = js_sys::Uint8Array::new(&data);
                let mut buf = vec![0u8; uint8.length() as usize];
                uint8.copy_to(&mut buf);
                log::debug!("[ws_onmessage] ArrayBuffer {} bytes", buf.len());
                queue_clone.borrow_mut().push_back(buf);
            } else if let Ok(blob) = event.data().dyn_into::<web_sys::Blob>() {
                // Handle Blob: call blob.arrayBuffer() to get Promise<ArrayBuffer>
                let queue_clone2 = Rc::clone(&queue_clone);
                let array_buffer_fn: js_sys::Function = js_sys::Reflect::get(&blob, &"arrayBuffer".into()).unwrap().into();
                let array_buffer_promise: js_sys::Promise = array_buffer_fn.call0(&blob).unwrap().into();
                let on_fulfilled = Closure::wrap(Box::new(move |array_buffer: JsValue| {
                    if let Ok(data) = array_buffer.dyn_into::<js_sys::ArrayBuffer>() {
                        let uint8 = js_sys::Uint8Array::new(&data);
                        let mut buf = vec![0u8; uint8.length() as usize];
                        uint8.copy_to(&mut buf);
                        log::debug!("[ws_onmessage] Blob {} bytes", buf.len());
                        queue_clone2.borrow_mut().push_back(buf);
                    }
                }) as Box<dyn FnMut(JsValue)>);
                array_buffer_promise.then(&on_fulfilled);
                on_fulfilled.forget();
            } else {
                log::debug!("[ws_onmessage] non-binary event: {:?}", event.data());
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // Wait for the connection to open.
        JsFuture::from(promise_from_event(&ws, "open"))
            .await
            .map_err(|e| format!("WebSocket open failed: {:?}", e))?;

        Ok((WsSender { ws }, WsReceiver { queue }))
    }

    fn promise_from_event(ws: &web_sys::WebSocket, event_name: &str) -> js_sys::Promise {
        js_sys::Promise::new(&mut |resolve, _| {
            let listener = Closure::once(move |_: web_sys::Event| {
                resolve.call0(&JsValue::NULL);
            });
            let _ = ws.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref());
            listener.forget();
        })
    }

    impl WsSender {
        pub async fn send(&self, data: &[u8]) -> Result<(), String> {
            self.ws
                .send_with_u8_array(data)
                .map_err(|e| format!("ws send failed: {:?}", e))?;
            Ok(())
        }
    }

    impl WsReceiver {
        pub async fn recv(&self) -> Result<Option<Vec<u8>>, String> {
            loop {
                if let Some(data) = self.queue.borrow_mut().pop_front() {
                    log::debug!("[ws_recv] queue pop {} bytes", data.len());
                    return Ok(Some(data));
                }
                sleep_ms(10).await;
            }
        }
    }
}

mod wt_conn {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::WebTransport;

    pub struct WtSender {
        writer: web_sys::WritableStreamDefaultWriter,
    }

    pub struct WtReceiver {
        reader: web_sys::ReadableStreamDefaultReader,
    }

    pub async fn connect_webtransport(
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
        pub async fn send(&self, data: &[u8]) -> Result<(), String> {
            let uint8 = js_sys::Uint8Array::from(data);
            JsFuture::from(self.writer.write_with_chunk(&uint8))
                .await
                .map_err(|e| format!("write failed: {:?}", e))?;
            Ok(())
        }
    }

    impl WtReceiver {
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
}

enum Transport {
    WebTransport,
    WebSocket,
}

const TRANSPORT_STORAGE_KEY: &str = "rterm_transport";

/// Get transport type from URL query parameter or sessionStorage.
/// Saves transport selection to sessionStorage so it persists across redirects.
fn get_transport_from_url() -> Transport {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return Transport::WebTransport,
    };
    let location = window.location();

    // First check URL query parameter
    if let Ok(search) = location.search() {
        if !search.is_empty() {
            if search.contains("transport=ws") || search.contains("transport=websocket") {
                // Save to sessionStorage for persistence across redirects
                if let Ok(Some(storage)) = window.session_storage() {
                    let _ = storage.set_item(TRANSPORT_STORAGE_KEY, "websocket");
                }
                return Transport::WebSocket;
            }
            if search.contains("transport=wt") || search.contains("transport=webtransport") {
                if let Ok(Some(storage)) = window.session_storage() {
                    let _ = storage.set_item(TRANSPORT_STORAGE_KEY, "webtransport");
                }
                return Transport::WebTransport;
            }
        }
    }

    // Fall back to sessionStorage (in case we were redirected and lost query params)
    if let Ok(Some(storage)) = window.session_storage() {
        if let Ok(Some(val)) = storage.get_item(TRANSPORT_STORAGE_KEY) {
            if val == "websocket" {
                return Transport::WebSocket;
            }
        }
    }

    Transport::WebTransport
}

/// Top-level connection loop with reconnection and exponential backoff.
pub async fn run_connection(shared: Rc<RefCell<Shared>>, ctx: egui::Context) {
    let location = web_sys::window().map(|w| w.location());
    let hostname = location
        .as_ref()
        .and_then(|l| l.hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());

    // Extract session name from URL path: /dev -> "dev", / -> ""
    let session_name = session::get_session_name_from_url();

    // Runtime transport selection
    let transport = get_transport_from_url();

    let (transport_name, port, path_prefix, url) = match transport {
        Transport::WebTransport => {
            let path = if session_name.is_empty() {
                "/wt".to_string()
            } else {
                format!("/wt/{}", session_name)
            };
            ("webtransport", 4433u16, "/wt", format!("https://{}:4433{}", hostname, path))
        }
        Transport::WebSocket => {
            let path = if session_name.is_empty() {
                "/ws".to_string()
            } else {
                format!("/ws/{}", session_name)
            };
            ("websocket", 4435u16, "/ws", format!("ws://{}:4435{}", hostname, path))
        }
    };

    let cert_hash =
        session::get_cert_hash_from_global().or_else(session::get_cert_hash_from_url);

    log::info!(
        "[rterm] connecting to {} (transport: {}, session: {})",
        url,
        transport_name,
        if session_name.is_empty() {
            "<auto>"
        } else {
            &session_name
        }
    );

    // Reconnection loop with exponential backoff.
    let mut backoff_ms = 1000u32;
    loop {
        match try_connect(&shared, &ctx, &url, cert_hash.as_deref(), &transport).await {
            Ok(()) => {
                // Session ended normally (PTY exited). Don't reconnect.
                log::info!("[rterm] session ended");
                break;
            }
            Err(e) => {
                shared.borrow_mut().connected = false;
                log::warn!(
                    "[rterm] disconnected: {}. Reconnecting in {}ms...",
                    e,
                    backoff_ms
                );
                ctx.request_repaint();
                sleep_ms(backoff_ms as i32).await;
                backoff_ms = (backoff_ms * 2).min(30000); // max 30s
            }
        }
    }
}

async fn try_connect(
    shared: &Rc<RefCell<Shared>>,
    ctx: &egui::Context,
    url: &str,
    cert_hash: Option<&[u8]>,
    transport: &Transport,
) -> Result<(), String> {
    match transport {
        Transport::WebTransport => try_connect_wt(shared, ctx, url, cert_hash).await,
        Transport::WebSocket => try_connect_ws(shared, ctx, url).await,
    }
}

async fn try_connect_ws(
    shared: &Rc<RefCell<Shared>>,
    ctx: &egui::Context,
    url: &str,
) -> Result<(), String> {
    let (sender, receiver) = ws_conn::connect_websocket(url).await?;
    let sender = Rc::new(sender);
    let receiver = Rc::new(RefCell::new(receiver));

    log::info!("[rterm] WebSocket connected");

    let (init_cols, init_rows) = shared.borrow().initial_size.unwrap_or((80, 24));
    let resize = encode_resize(init_cols as u16, init_rows as u16);
    sender
        .send(&encode_message(&resize))
        .await
        .map_err(|e| format!("send resize: {}", e))?;

    shared.borrow_mut().connected = true;
    log::info!("[rterm] session active");
    ctx.request_repaint();

    // Send loop.
    let shared_send = Rc::clone(shared);
    let sender_clone = Rc::clone(&sender);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let msg = shared_send
                .try_borrow_mut()
                .ok()
                .and_then(|mut s| s.send_queue.pop_front());
            if let Some(msg) = msg {
                if let Err(e) = sender_clone.send(&msg).await {
                    log::error!("[rterm] send error: {}", e);
                    return;
                }
            } else {
                sleep_ms(10).await;
            }
        }
    });

    // Receive loop.
    let recv = Rc::clone(&receiver);
    let mut recv_buf = RecvBuffer::new();
    loop {
        let opt_data = recv.borrow_mut().recv().await?;
        let data = match opt_data {
            Some(d) => d,
            None => {
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return Err("connection closed".into());
            }
        };
        log::debug!("[rterm] recv {} bytes", data.len());
        recv_buf.push(&data);
        let mut s = loop {
            match shared.try_borrow_mut() {
                Ok(s) => break s,
                Err(_) => sleep_ms(1).await,
            }
        };
        while let Some(msg_bytes) = recv_buf.try_read_message() {
            log::debug!("[rterm] decoded msg, {} bytes", msg_bytes.len());
            match decode_server_msg(&msg_bytes) {
                Ok(ServerMsg::ScreenSnapshot(ref sd)) => {
                    log::debug!("[rterm] ScreenSnapshot received");
                    s.grid.apply_snapshot(sd);
                }
                Ok(ServerMsg::ScreenUpdate(ref sd)) => {
                    log::debug!("[rterm] ScreenUpdate received");
                    s.grid.apply_update(sd);
                }
                Ok(ServerMsg::Exit(_)) => {
                    s.connected = false;
                    drop(s);
                    ctx.request_repaint();
                    return Ok(());
                }
                Ok(ServerMsg::Error(msg)) => {
                    log::error!("[rterm] error: {}", msg);
                }
                Ok(ServerMsg::Bell) => {}
                Ok(ServerMsg::Scrollback(sd)) => s.grid.apply_scrollback(&sd),
                Err(e) => {
                    log::error!("[rterm] decode error: {}", e);
                }
            }
        }
        // Always repaint after processing data.
        drop(s);
        ctx.request_repaint();
        // Always yield to let UI paint before next recv.
        sleep_ms(1).await;
    }
}

async fn try_connect_wt(
    shared: &Rc<RefCell<Shared>>,
    ctx: &egui::Context,
    url: &str,
    cert_hash: Option<&[u8]>,
) -> Result<(), String> {
    // Keep WebTransport alive - it must not be dropped while streams are in use.
    let (sender, receiver, transport) = wt_conn::connect_webtransport(url, cert_hash).await?;
    let transport = Rc::new(transport);

    log::info!("[rterm] WebTransport connected");

    let (init_cols, init_rows) = shared.borrow().initial_size.unwrap_or((80, 24));
    let resize = encode_resize(init_cols as u16, init_rows as u16);
    sender
        .send(&encode_message(&resize))
        .await
        .map_err(|e| format!("send resize: {}", e))?;

    shared.borrow_mut().connected = true;
    log::info!("[rterm] session active");
    ctx.request_repaint();

    // Send loop.
    let shared_send = Rc::clone(shared);
    let sender = Rc::new(sender);
    let sender_clone = Rc::clone(&sender);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let msg = shared_send
                .try_borrow_mut()
                .ok()
                .and_then(|mut s| s.send_queue.pop_front());
            if let Some(msg) = msg {
                if let Err(e) = sender_clone.send(&msg).await {
                    log::error!("[rterm] send error: {}", e);
                    return;
                }
            } else {
                sleep_ms(10).await;
            }
        }
    });

    // Receive loop - keep transport alive while receiving.
    let receiver = Rc::new(receiver);
    let transport_recv = Rc::clone(&transport);
    let mut recv_buf = RecvBuffer::new();
    loop {
        // Use the transport in the loop to ensure it stays alive.
        let _ = &transport_recv;
        match receiver.recv().await {
            Ok(Some(data)) => {
                log::debug!("[rterm] recv {} bytes", data.len());
                recv_buf.push(&data);
                let mut s = loop {
                    match shared.try_borrow_mut() {
                        Ok(s) => break s,
                        Err(_) => sleep_ms(1).await,
                    }
                };
                while let Some(msg_bytes) = recv_buf.try_read_message() {
                    log::debug!("[rterm] decoded msg {} bytes", msg_bytes.len());
                    match decode_server_msg(&msg_bytes) {
                        Ok(ServerMsg::ScreenSnapshot(sd)) => {
                            log::debug!("[rterm] ScreenSnapshot");
                            s.grid.apply_snapshot(&sd);
                        }
                        Ok(ServerMsg::ScreenUpdate(sd)) => {
                            log::debug!("[rterm] ScreenUpdate");
                            s.grid.apply_update(&sd);
                        }
                        Ok(ServerMsg::Exit(_)) => {
                            s.connected = false;
                            drop(s);
                            ctx.request_repaint();
                            return Ok(());
                        }
                        Ok(ServerMsg::Error(msg)) => {
                            log::error!("[rterm] error: {}", msg);
                        }
                        Ok(ServerMsg::Bell) => {}
                        Ok(ServerMsg::Scrollback(sd)) => {
                            s.grid.apply_scrollback(&sd);
                        }
                        Err(e) => {
                            log::error!("[rterm] decode error: {}", e);
                        }
                    }
                }
                drop(s);
                // Always repaint when we've processed data (complete or partial).
                // This ensures the UI updates even if buffer has incomplete messages waiting for more data.
                ctx.request_repaint();
                // Yield to let UI paint before next recv.
                sleep_ms(1).await;
            }
            Ok(None) => {
                log::warn!("[rterm] WebTransport stream closed");
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return Err("connection closed".into());
            }
            Err(e) => {
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return Err(format!("recv error: {}", e));
            }
        }
    }
}

/// Async sleep using browser setTimeout.
pub async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
