# rterm TODO

## Polish & UX
- [x] Persistent TLS cert (save to ~/.config/rterm/, survives restarts)
- [x] Scrollback retrieval from server
- [x] Text selection + clipboard copy in WASM thin renderer
- [x] Paste support (bracketed paste wrapping server-side)

## Rendering
- [x] Wide character (CJK) support — double-width cells
- [x] Bold font variant (faux bold via double-draw)
- [x] Cursor styles (bar, underline, block)

## Architecture
- [x] Wire wt_handler and service to call session::run_session in production
- [ ] rterm-shell (native WebView wrapper — Phase 4)
- [ ] Reconnection on disconnect
- [ ] Multiple concurrent sessions on one relay
- [ ] Session persistence (resume after page reload)

## Coverage
- [ ] Push from 79% toward 85% (h3 stream trait for serve_file)

## Not doing
- ~~Cloudflare tunnel~~ — cannot tunnel WebTransport (QUIC/UDP)
