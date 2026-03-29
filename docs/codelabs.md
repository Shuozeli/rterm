<!-- agent-updated: 2026-03-29T23:20:00Z -->
# rterm Codelabs

## Codelab 1: Screen Buffer Basics

Build Cell and ScreenBuffer types. Write chars, move cursor, apply SGR attributes. Assert cell contents in tests. No parser, no GUI -- just the data model.

## Codelab 2: VT Parser Integration

Integrate vte crate. Feed raw ANSI byte sequences through the parser, dispatch to ScreenBuffer, assert cells have correct characters and colors.

## Codelab 3: Full VT Emulation

Process captured terminal output (ls --color, vim startup). Verify screen state matches expected output. Test scroll regions, alternate screen, cursor movement edge cases.

## Codelab 4: FlatBuffers Protocol

Define the rterm protocol schema in FlatBuffers. Compile with flatbuffers-rs. Write round-trip serialization tests for all message types.

## Codelab 5: gRPC over HTTP/3

Add HTTP/3 support to pure-grpc-rs using quinn. Build TerminalService with bidi streaming. Connect a test client over QUIC, verify PTY interaction works.

## Codelab 6: WebTransport Client

Build WebTransport client for WASM using raw bidi streams with length-prefixed FlatBuffers (not gRPC). Connect from browser to rterm-relay over HTTP/3. Verify bidi streaming works from browser.

## Codelab 7: egui Grid in Browser

Compile egui to WASM. Render a hardcoded 80x24 grid of styled cells. No PTY connection -- just prove the rendering works in a browser.

## Codelab 8: Interactive Browser Terminal

Connect Codelab 7 (egui WASM) to Codelab 6 (WebTransport gRPC). Full interactive shell session in the browser over HTTP/3.

## Codelab 9: Native Desktop Shell

Build rterm-shell with wry. Embed the WASM bundle. Local PTY with gRPC/HTTP/3 server (quinn). Launch and get a working desktop terminal.

## Codelab 10: Custom Glyph Atlas

Replace egui built-in text with rustybuzz + fontdue glyph atlas. Verify ligatures render correctly.

## Codelab 11: Server-Side VT Emulation + Screen Diffing

Understand how the relay server runs the VT emulator and sends typed screen updates. Walk through session::run_session: PTY spawn, Terminal.feed(), PrevScreen.diff(), ScreenUpdate encoding. Compare against the naive approach of sending raw PTY bytes.
