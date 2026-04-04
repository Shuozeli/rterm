//! SGR-mode VT mouse encoding for WebTransport and WebSocket handlers.
//!
//! Both handlers use identical SGR (Simple Graphics Replacement) encoding:
//! `CSI < Pb ; Px ; Py M` for press/drag, `CSI < Pb ; Px ; Py m` for release.
//!
//! Button encoding: 0=left, 1=middle, 2=right, 3=release, 4=scroll-up, 5=scroll-down.
//! Add 32 for drag events. Add modifiers: 4=Shift, 8=Meta, 16=Control.

use rterm_proto::MouseEvent;

/// Encode a MouseEvent as a VT mouse protocol sequence (SGR mode).
pub fn encode_vt_mouse(event: &MouseEvent) -> Vec<u8> {
    // SGR mode (1006): CSI < Pb ; Px ; Py M for press, CSI < Pb ; Px ; Py m for release
    // Button encoding:
    //   0 = left press, 1 = middle press, 2 = right press
    //   3 = release
    //   4 = scroll up, 5 = scroll down
    //   Add 32 for drag (e.g., 32 = left drag)
    //   Add modifiers: 4 = Shift, 8 = Meta, 16 = Control

    let (button, suffix) = match event.kind {
        0 => (event.button, b'M'),      // Press
        1 => (event.button + 3, b'm'),  // Release (button 3 = release)
        2 => (event.button + 32, b'M'), // Drag
        3 => (4, b'M'),                 // Scroll up
        4 => (5, b'M'),                 // Scroll down
        _ => return Vec::new(),
    };

    let modifiers = event.modifiers;
    let final_button = button | ((modifiers & 0x07) << 2);

    // Encode as UTF-8 string: "CSI < B ; X ; Y M" or "CSI < B ; X ; Y m"
    let x = event.col + 1; // 1-indexed
    let y = event.row + 1; // 1-indexed

    let mut buf = Vec::with_capacity(16);
    buf.push(0x1b); // ESC
    buf.push(0x5b); // [
    buf.push(b'<');

    // Encode button/modifiers
    let button_str = format!("{}", final_button);
    buf.extend_from_slice(button_str.as_bytes());
    buf.push(b';');

    // Encode x
    let x_str = format!("{}", x);
    buf.extend_from_slice(x_str.as_bytes());
    buf.push(b';');

    // Encode y
    let y_str = format!("{}", y);
    buf.extend_from_slice(y_str.as_bytes());

    buf.push(suffix);

    buf
}
