/// Connection management: WebTransport connect, send/recv loops, reconnection with backoff.
use crate::app::Shared;
use crate::messages::{decode_server_msg, encode_resize, ServerMsg};
use crate::protocol::{encode_message, RecvBuffer};
use crate::session;
use crate::transport;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;

/// Top-level connection loop with reconnection and exponential backoff.
pub async fn run_connection(shared: Rc<RefCell<Shared>>, ctx: egui::Context) {
    let location = web_sys::window().map(|w| w.location());
    let hostname = location
        .as_ref()
        .and_then(|l| l.hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let port = location
        .as_ref()
        .and_then(|l| l.port().ok())
        .and_then(|p| if p.is_empty() { None } else { Some(p) })
        .unwrap_or_else(|| "4433".to_string());

    // Extract session name from URL path: /dev -> "dev", / -> ""
    let session_name = session::get_session_name_from_url();
    let wt_path = if session_name.is_empty() {
        "/wt".to_string()
    } else {
        format!("/wt/{}", session_name)
    };
    let url = format!("https://{}:{}{}", hostname, port, wt_path);
    let cert_hash =
        session::get_cert_hash_from_global().or_else(session::get_cert_hash_from_url);

    web_sys::console::log_1(
        &format!(
            "[rterm] connecting to {} (session: {})",
            url,
            if session_name.is_empty() {
                "<auto>"
            } else {
                &session_name
            }
        )
        .into(),
    );

    // Reconnection loop with exponential backoff.
    let mut backoff_ms = 1000u32;
    loop {
        match try_connect(&shared, &ctx, &url, cert_hash.as_deref()).await {
            Ok(()) => {
                // Session ended normally (PTY exited). Don't reconnect.
                web_sys::console::log_1(&"[rterm] session ended".into());
                break;
            }
            Err(e) => {
                shared.borrow_mut().connected = false;
                web_sys::console::warn_1(
                    &format!(
                        "[rterm] disconnected: {}. Reconnecting in {}ms...",
                        e, backoff_ms
                    )
                    .into(),
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
) -> Result<(), String> {
    let (sender, receiver, _transport) = transport::connect(url, cert_hash).await?;

    web_sys::console::log_1(&"[rterm] WebTransport connected!".into());

    let (init_cols, init_rows) = shared.borrow().initial_size.unwrap_or((80, 24));
    let resize = encode_resize(init_cols as u16, init_rows as u16);
    sender
        .send(&encode_message(&resize))
        .await
        .map_err(|e| format!("send resize: {}", e))?;

    shared.borrow_mut().connected = true;
    web_sys::console::log_1(&"[rterm] session active".into());
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
                    web_sys::console::log_1(
                        &format!("[rterm] send error: {}", e).into(),
                    );
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
                        Ok(ServerMsg::ScreenSnapshot(sd)) => {
                            s.grid.apply_snapshot(&sd);
                        }
                        Ok(ServerMsg::ScreenUpdate(sd)) => {
                            s.grid.apply_update(&sd);
                        }

                        Ok(ServerMsg::Exit(_)) => {
                            s.connected = false;
                            ctx.request_repaint();
                            return Ok(()); // Normal exit -- don't reconnect.
                        }
                        Ok(ServerMsg::Error(msg)) => {
                            web_sys::console::error_1(
                                &format!("[rterm] error: {}", msg).into(),
                            );
                        }
                        Ok(ServerMsg::Bell) => {}
                        Err(e) => {
                            web_sys::console::log_1(
                                &format!("[rterm] decode error: {}", e).into(),
                            );
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
