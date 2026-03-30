# rterm TODO

## Polish & UX
- [ ] Persistent TLS cert (save to ~/.config/rterm/, survives restarts)
- [ ] Scrollback retrieval from server (protocol defined, wire it up)
- [ ] Text selection + clipboard copy in WASM thin renderer
- [ ] Paste support (bracketed paste wrapping server-side)

## Rendering
- [ ] Wide character (CJK) support — double-width cells
- [ ] Bold font variant
- [ ] Cursor styles (bar, underline, blink)

## Architecture
- [ ] Wire wt_handler and service to call session::run_session in production
- [ ] rterm-shell (native WebView wrapper — Phase 4)
- [ ] Reconnection on disconnect
- [ ] Multiple concurrent sessions on one relay
- [ ] Session persistence (resume after page reload)

## Coverage
- [ ] Push from 79% toward 85% (h3 stream trait for serve_file)

## Not doing
- ~~Cloudflare tunnel~~ — cannot tunnel WebTransport (QUIC/UDP)
