/// Session name and certificate hash extraction from the browser URL/globals.
use wasm_bindgen::prelude::*;

/// Extract session name from URL path: "/wt/dev" -> "dev", "/ws/test" -> "test", "/" -> ""
pub fn get_session_name_from_url() -> String {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return String::new(),
    };
    let path = window.location().pathname().unwrap_or_default();
    // Strip leading slash, "index.html", and the transport prefix (/wt/ or /ws/).
    let name = path.trim_start_matches('/');
    let name = name
        .strip_prefix("wt/")
        .or_else(|| name.strip_prefix("ws/"))
        .unwrap_or(name);
    let name = name
        .strip_suffix("index.html")
        .unwrap_or(name)
        .trim_end_matches('/');
    name.to_string()
}

/// Try to read cert hash from the global `__RTERM_CERT_HASH__` variable injected by the server.
pub fn get_cert_hash_from_global() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let hash_js = js_sys::Reflect::get(&window, &"__RTERM_CERT_HASH__".into()).ok()?;
    let hash_str = hash_js.as_string()?;
    if hash_str.is_empty() {
        return None;
    }
    log::info!("[rterm] cert hash from server: {}", hash_str);
    let atob_fn: js_sys::Function =
        js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
    let decoded_js = atob_fn
        .call1(&JsValue::NULL, &hash_str.into())
        .ok()?;
    let decoded_str: js_sys::JsString = decoded_js.into();
    Some(
        (0..decoded_str.length())
            .map(|i| decoded_str.char_code_at(i) as u8)
            .collect(),
    )
}

/// Try to read cert hash from the `?cert=` URL query parameter.
pub fn get_cert_hash_from_url() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    if search.is_empty() {
        return None;
    }
    for param in search.trim_start_matches('?').split('&') {
        if let Some(value) = param.strip_prefix("cert=") {
            let decoded = js_sys::decode_uri_component(value).ok()?;
            let hash_str: String = decoded.into();
            let atob_fn: js_sys::Function =
                js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
            let decoded_js = atob_fn
                .call1(&JsValue::NULL, &hash_str.into())
                .ok()?;
            let decoded_str: js_sys::JsString = decoded_js.into();
            return Some(
                (0..decoded_str.length())
                    .map(|i| decoded_str.char_code_at(i) as u8)
                    .collect(),
            );
        }
    }
    None
}
