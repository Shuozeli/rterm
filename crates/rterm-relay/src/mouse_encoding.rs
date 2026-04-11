//! SGR-mode VT mouse encoding for WebTransport and WebSocket handlers.
//!
//! Both handlers use identical SGR (Simple Graphics Replacement) encoding:
//! `CSI < Pb ; Px ; Py M` for press/drag, `CSI < Pb ; Px ; Py m` for release.
//!
//! Button encoding: 0=left, 1=middle, 2=right, 3=release, 4=scroll-up, 5=scroll-down.
//! Add 32 for drag events. Add modifiers: 4=Shift, 8=Meta, 16=Control.

use rterm_proto::MouseEvent;

/// Encode a non-negative integer to decimal bytes, returning the count.
/// Writes digits right-to-left into `buf` (which must have capacity).
fn write_decimal(val: u16, buf: &mut [u8]) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut v = val;
    let mut len = 0;
    while v > 0 {
        buf[5 - len] = b'0' + (v % 10) as u8;
        v /= 10;
        len += 1;
    }
    // Shift digits to the start of the buffer.
    for i in 0..len {
        buf[i] = buf[5 - len + 1 + i];
    }
    len
}

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

    // Encode button/modifiers (max 2 digits for button 0-63)
    let mut decimal_buf = [0u8; 6];
    let len = write_decimal(final_button as u16, &mut decimal_buf);
    buf.extend_from_slice(&decimal_buf[..len]);
    buf.push(b';');

    // Encode x
    let len = write_decimal(x, &mut decimal_buf);
    buf.extend_from_slice(&decimal_buf[..len]);
    buf.push(b';');

    // Encode y
    let len = write_decimal(y, &mut decimal_buf);
    buf.extend_from_slice(&decimal_buf[..len]);

    buf.push(suffix);

    buf
}
