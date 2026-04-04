# Postmortem: WebSocket Transport - Blank Screen

**Date:** 2026-04-04
**Duration:** ~2 hours
**Impact:** WebSocket transport (`wss://`) rendered blank terminal, WebTransport (`quic://`) worked fine.

---

## Root Causes

### 1. Length-Prefix Mismatch on Server → Client Messages

**Problem:** The server's `ws_handler` was sending `ScreenSnapshot` and `Exit` messages as raw FlatBuffers bytes without the 4-byte length prefix that the client expects.

**Code change:**
```rust
// BEFORE (broken):
let encoded = ServerMsg::ScreenSnapshot(snapshot).encode_flatbuffer();
ws_sink.send(tungstenite::Message::Binary(encoded.into())).await?;

// AFTER (fixed):
let encoded = encode_message(ServerMsg::ScreenSnapshot(snapshot).encode_flatbuffer());
ws_sink.send(tungstenite::Message::Binary(encoded.into())).await?;
```

**The `encode_message` helper:**
```rust
fn encode_message(payload: Vec<u8>) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&payload);
    buf
}
```

**Why it worked for WebTransport:** The `wt_handler` used `read_message()`/`write_message()` helpers that handled length-prefixing at the stream level. The `ws_handler` was a separate implementation that didn't use these helpers.

---

### 2. Client-Side Blob Handling

**Problem:** Browser WebSocket API can return `Blob` objects instead of `ArrayBuffer` for binary messages, depending on the browser/frame size.

**Symptom:** `[ws_onmessage] non-binary event: JsValue(Blob)` in browser console.

**Code change:**
```rust
// BEFORE (broken - only handled ArrayBuffer):
if let Ok(data) = event.data().dyn_into::<js_sys::ArrayBuffer>() { ... }

// AFTER (fixed - handles both):
if let Ok(data) = event.data().dyn_into::<js_sys::ArrayBuffer>() { ... }
else if let Ok(blob) = event.data().dyn_into::<web_sys::Blob>() {
    let array_buffer_fn: js_sys::Function = js_sys::Reflect::get(&blob, &"arrayBuffer".into()).unwrap().into();
    let array_buffer_promise: js_sys::Promise = array_buffer_fn.call0(&blob).unwrap().into();
    let on_fulfilled = Closure::wrap(Box::new(move |array_buffer: JsValue| {
        if let Ok(data) = array_buffer.dyn_into::<js_sys::ArrayBuffer>() {
            let uint8 = js_sys::Uint8Array::new(&data);
            let mut buf = vec![0u8; uint8.length() as usize];
            uint8.copy_to(&mut buf);
            queue_clone2.borrow_mut().push_back(buf);
        }
    }) as Box<dyn FnMut(JsValue)>);
    array_buffer_promise.then(&on_fulfilled);
    on_fulfilled.forget();
}
```

---

## Lessons Learned

1. **Wire format must be consistent across all transport implementations.** WebTransport and WebSocket share the same FlatBuffers protocol but had different framing implementations.

2. **Test with actual binary protocols.** The `[ws_onmessage] non-binary event: Blob` error only surfaces in a real browser, not in unit tests.

3. **Add transport-agnostic message framing.** Consider extracting `read_message`/`write_message` into a shared module so all transports use the same length-prefix framing.

---

## Files Changed

- `crates/rterm-relay/src/ws_handler.rs` — Added `encode_message()`/`strip_length_prefix()`, applied to all send/receive paths
- `crates/rterm-wasm/src/connection.rs` — Added Blob→ArrayBuffer conversion in WebSocket `onmessage` handler

<!-- agent-updated: 2026-04-04T01:45:00Z -->
