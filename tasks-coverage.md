# Coverage Tasks

Target: ~85% lib coverage
Current: 77.5% (245 tests)

## Low effort — more tests with existing fakes

- [x] session.rs: KeyInput forwarded to PTY stdin
- [x] session.rs: PasteInput forwarded to PTY stdin
- [x] session.rs: Resize forwarded to PTY resize channel
- [x] session.rs: MouseEvent silently ignored (no stdin data)
- [x] session.rs: client disconnect mid-session graceful shutdown
- [x] session.rs: SessionError display for all variants
- [x] terminal.rs: CSI d (VPA)
- [x] terminal.rs: CSI G (CHA)
- [x] terminal.rs: ESC M (reverse index)
- [x] terminal.rs: CSI S / CSI T (scroll up/down)
- [x] terminal.rs: origin mode (DECOM)
- [x] terminal.rs: ESC D (index)
- [x] pty.rs: FakePtyControl reads stdin
- [x] pty.rs: FakePtyControl reads resize

## Medium effort — trait extraction + refactor

- [ ] Refactor wt_handler to call session::run_session
- [ ] Refactor service.rs to call session::run_session
- [ ] Extract h3 stream trait for serve_file testability

## Coverage summary

| File | Coverage |
|---|---|
| cell.rs | 100% |
| color.rs | 100% |
| buffer.rs | 99% |
| terminal.rs | 98% |
| session.rs | 96% |
| proto lib.rs | 95% |
| screen_diff.rs | 94% |
| static_files.rs | 70% |
| pty.rs | 64% |
| https_server.rs | 64% |
| wt_handler.rs | 54% |
| service.rs | 8% (covered by 20 integration tests) |
