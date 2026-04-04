/// Connection management: WebTransport connect, send/recv loops, reconnection with backoff.
/// The transport is determined at compile time via cargo features:
/// - `transport-webtransport` (default): connects via WebTransport on port 4433, path /wt/{session}
/// - `transport-websocket`: connects via WebSocket on port 4435, path /ws/{session}
use crate::app::Shared;
use crate::messages::{decode_server_msg, encode_resize, ServerMsg};
use crate::protocol::{encode_message, RecvBuffer};
use crate::session;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;

/// Compile-time transport configuration.
#[cfg(feature = "transport-websocket")]
const TRANSPORT_NAME: &str = "websocket";
#[cfg(feature = "transport-websocket")]
const TRANSPORT_PORT: u16 = 4435;
#[cfg(feature = "transport-websocket")]
const TRANSPORT_PATH_PREFIX: &str = "/ws";

#[cfg(not(feature = "transport-websocket"))]
const TRANSPORT_NAME: &str = "webtransport";
#[cfg(not(feature = "transport-websocket"))]
const TRANSPORT_PORT: u16 = 4433;
#[cfg(not(feature = "transport-websocket"))]
const TRANSPORT_PATH_PREFIX: &str = "/wt";

/// Top-level connection loop with reconnection and exponential backoff.
pub async fn run_connection(shared: Rc<RefCell<Shared>>, ctx: egui::Context) {
    let location = web_sys::window().map(|w| w.location());
    let hostname = location
        .as_ref()
        .and_then(|l| l.hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());

    // Extract session name from URL path: /dev -> "dev", / -> ""
    let session_name = session::get_session_name_from_url();

    // Build the relay URL based on compile-time transport feature.
    let path = if session_name.is_empty() {
        TRANSPORT_PATH_PREFIX.to_string()
    } else {
        format!("{}/{}", TRANSPORT_PATH_PREFIX, session_name)
    };
    let url = format!("wss://{}:{}{}", hostname, TRANSPORT_PORT, path);

    let cert_hash =
        session::get_cert_hash_from_global().or_else(session::get_cert_hash_from_url);

    log::info!(
        "[rterm] connecting to {} (transport: {}, session: {})",
        url,
        TRANSPORT_NAME,
        if session_name.is_empty() {
            "<auto>"
        } else {
            &session_name
        }
    );

    // Reconnection loop with exponential backoff.
    let mut backoff_ms = 1000u32;
    loop {
        match try_connect(&shared, &ctx, &url, cert_hash.as_deref()).await {
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

#[cfg(feature = "transport-websocket")]
mod ws_conn {
    use super::*;
    use std::collections::VecDeque;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

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
            ws.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref());
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

#[cfg(feature = "transport-websocket")]
use ws_conn::*;

#[cfg(feature = "transport-websocket")]
async fn try_connect(
    shared: &Rc<RefCell<Shared>>,
    ctx: &egui::Context,
    url: &str,
    _cert_hash: Option<&[u8]>,
) -> Result<(), String> {
    let (sender, receiver) = connect_websocket(url).await?;
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
        ctx.request_repaint();
    }
}

#[cfg(not(feature = "transport-websocket"))]
use crate::transport;

#[cfg(not(feature = "transport-websocket"))]
async fn try_connect(
    shared: &Rc<RefCell<Shared>>,
    ctx: &egui::Context,
    url: &str,
    cert_hash: Option<&[u8]>,
) -> Result<(), String> {
    let (sender, receiver, _) = transport::connect(url, cert_hash).await?;

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

    // Receive loop.
    let mut recv_buf = RecvBuffer::new();
    loop {
        match receiver.recv().await {
            Ok(Some(data)) => {
                recv_buf.push(&data);
                let mut s = loop {
                    match shared.try_borrow_mut() {
                        Ok(s) => break s,
                        Err(_) => sleep_ms(1).await,
                    }
                };
                while let Some(msg_bytes) = recv_buf.try_read_message() {
                    match decode_server_msg(&msg_bytes) {
                        Ok(ServerMsg::ScreenSnapshot(sd)) => s.grid.apply_snapshot(&sd),
                        Ok(ServerMsg::ScreenUpdate(sd)) => s.grid.apply_update(&sd),
                        Ok(ServerMsg::Exit(_)) => {
                            s.connected = false;
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
                ctx.request_repaint();
            }
            Ok(None) => {
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
