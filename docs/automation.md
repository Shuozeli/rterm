<!-- agent-updated: 2026-04-10T00:00:00Z -->
# rterm Automation API

Playwright-style headless terminal automation over gRPC. Every operation is a
unary RPC — no streaming required for automation clients.

---

## Mental model

```
browser (egui/WASM)         automation client (rterm-cli / SDK)
        │                               │
        │  bidi-stream                  │  unary RPCs
        └──────────┐   ┌───────────────┘
                   ▼   ▼
             TerminalServer (relay)
                   │
            SessionManager
                   │
             ManagedSession ── VT emulator ── PTY
```

Automation clients share the same sessions as browser clients. A session
created with `create` can be attached to by a browser later, and vice versa.

---

## Current API surface

| Command | gRPC method | Description |
|---------|------------|-------------|
| `list` | `ListActiveSessions` | List all live sessions |
| `create` | `CreateSession` | Explicit session creation (shell, cols, rows) |
| `kill` | `KillSession` | Destroy a session and its PTY |
| `resize` | `ResizeSession` | Resize the terminal |
| `type` | `TypeAction` | Send UTF-8 text to the PTY |
| `send-keys` | `SendKeys` | Send raw PTY bytes (escape sequences) |
| `get-text` | `GetSnapshot` | Plain-text screen dump |
| `snapshot` | `GetSnapshot` | Structured cell snapshot |
| `wait` | `WaitForText` | Block until a substring appears (server-side poll) |
| `run` | `RunCommand` | Send a command + wait for shell sentinel |

### `run` internals

`RunCommand` wraps the user command in a sentinel:
```
<command>; echo "RTERM_DONE_<nanos>"
```
Then polls the VT screen every 100 ms until the sentinel appears. This works
reliably for non-interactive commands that return to the shell prompt.

**It does NOT work for:**
- Commands that launch interactive TUIs (vim, less, htop, fzf)
- REPLs (python3, node, psql)
- Commands that swallow stdin before exiting
- Any command that runs longer than `timeout_ms`

---

### 6. `exec` — Ephemeral command execution (streaming)

Like SSH's `exec` channel: run a single command in a specified working directory,
stream stdout/stderr back as it's produced, return exit code when done.

**Unlike `run`** — no sentinel wrapping, no VT screen polling, no session persistence.
Each `exec` is independent: spawn PTY → run command → stream output → exit.

```
rterm-cli exec --cwd /path/to/dir -- echo hello
# Output streams to stdout in real-time
# Exit code propagates to CLI exit code
```

**gRPC: server-streaming RPC** — chunks stream back as data arrives, not buffered until end.

#### Request

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | Command to run (e.g., `"ls -la"`) |
| `cwd` | string | Working directory (e.g., `"/home/user"`) |
| `timeout_ms` | uint64 | Max execution time (0 = 30s default) |

#### Response (streamed chunks)

| Field | Type | Description |
|-------|------|-------------|
| `stdout` | [ubyte] | Chunk of stdout bytes (empty if no stdout this chunk) |
| `stderr` | [ubyte] | Chunk of stderr bytes (always empty — see note) |
| `exit_code` | int32 | Present only in FINAL chunk |
| `timed_out` | bool | Present only in FINAL chunk |

> **Note on stderr:** PTY naturally merges stdout and stderr into a single stream.
> All output arrives via `stdout`. The `stderr` field is always empty. This is
> standard PTY behavior — if you need stderr separation, redirect in the command
> itself (e.g., `exec -- bash -c "cmd 2>/tmp/stderr"`).

#### Internals

```
Client                          Server
  │                               │
  │──── ExecRequest ─────────────►│  {command: "ls -la", cwd: "/tmp"}
  │                               │
  │◄─── ExecResponse ─────────────│  {stdout: [0x6c, 0x73, ...]}  (first chunk)
  │◄─── ExecResponse ─────────────│  {stdout: [...]}  (more chunks)
  │◄─── ExecResponse ─────────────│  {stdout: [], exit_code: 0}  (final)
  │                               │
```

1. `spawn_exec(command, cwd)` opens a new PTY pair
2. Child spawned as `bash -c "cd <cwd> && <command>"`
3. Background thread reads PTY master stdout, sends chunks via channel
4. When child exits, exit code sent and channels closed
5. Handler uses `tokio::time::timeout` — on timeout, child killed and final chunk sent with `timed_out: true`

#### Timeout behavior

| Scenario | Behavior |
|----------|----------|
| Command completes normally | Final chunk has `exit_code: N`, `timed_out: false` |
| Command times out | Final chunk has `exit_code: -1`, `timed_out: true`, child killed |
| PTY spawn failure | gRPC `Status::internal` error |
| Client disconnects | PTY child dropped, resources cleaned up |

#### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1-255 | Command exit code |
| 124 | Timeout (standard `timeout` convention) |
| -1 | Abnormal (killed, spawn failure) |

---

## Proposed extensions

### 1. `press` — Named key press

`send-keys` requires knowing raw PTY escape sequences. `press` maps human names
to the correct byte sequences, making scripts readable.

```
rterm-cli press <session> <key> [<key> ...]
```

**Key name table:**

| Name(s) | Bytes | Use case |
|---------|-------|----------|
| `Enter` | `\r` | Confirm, submit |
| `Escape`, `Esc` | `\x1b` | Exit insert mode, cancel |
| `Tab` | `\t` | Completion, indent |
| `Backspace` | `\x7f` | Delete char left |
| `Delete` | `\x1b[3~` | Delete char right |
| `Up`, `ArrowUp` | `\x1b[A` | Navigate up |
| `Down`, `ArrowDown` | `\x1b[B` | Navigate down |
| `Right`, `ArrowRight` | `\x1b[C` | Navigate right |
| `Left`, `ArrowLeft` | `\x1b[D` | Navigate left |
| `Home` | `\x1b[H` | Jump to line start |
| `End` | `\x1b[F` | Jump to line end |
| `PageUp` | `\x1b[5~` | Scroll up one page |
| `PageDown` | `\x1b[6~` | Scroll down one page |
| `Ctrl+C`, `C-c` | `\x03` | Interrupt (SIGINT) |
| `Ctrl+D`, `C-d` | `\x04` | EOF / exit shell |
| `Ctrl+Z`, `C-z` | `\x1a` | Suspend (SIGTSTP) |
| `Ctrl+L`, `C-l` | `\x0c` | Clear screen |
| `Ctrl+A`, `C-a` | `\x01` | Jump to line start (readline) |
| `Ctrl+E`, `C-e` | `\x05` | Jump to line end (readline) |
| `Ctrl+U`, `C-u` | `\x15` | Clear line (readline) |
| `F1`–`F12` | `\x1bOP`… | Function keys |

Multiple keys in one call are sent as a single write:
```
rterm-cli press myses Escape Escape   # two Escapes
rterm-cli press myses Ctrl+C          # SIGINT
```

---

### 2. `assert` — Non-blocking text assertion

`wait` always polls until a timeout. `assert` checks the screen right now
and exits non-zero immediately if the pattern is absent. Useful for CI
assertions after a `wait` has already confirmed the state.

```
rterm-cli assert <session> <pattern>
```

Exit codes:
- `0` — pattern found on current screen
- `1` — pattern not found

---

### 3. `exec` — Launch interactive program in session (non-blocking)

> **Note:** This `exec` is different from the ephemeral streaming `exec` (section 6).
> This one runs a command *inside* the existing PTY session, leaving it running.
> The streaming `exec` (section 6) spawns a fresh PTY for a single command.

`run` is blocking and wraps commands with a sentinel, which is wrong for TUIs.
`exec` sends the command + Enter but returns immediately, leaving the program
running in the session. The caller then uses `wait` / `press` / `type` to
interact with it.

```
rterm-cli exec <session> <command>
```

This is equivalent to `type <session> "<command>\n"` but makes the intent
explicit in scripts.

```bash
# Start vim and open a file
rterm-cli exec myses "vim notes.txt"
rterm-cli wait myses "~"                 # vim shows ~ for empty lines
```

---

### 4. `cursor` — Get cursor position

Returns the current cursor row and column, and whether the cursor is visible.
Useful for asserting that a TUI positioned the cursor correctly.

```
rterm-cli cursor <session>
# output: row=5 col=12 visible=true
```

---

### 5. `snapshot-json` — Machine-readable cell dump

`snapshot` currently prints `{:#?}` Rust debug. A `snapshot-json` command
emits proper JSON for use in scripts and test frameworks:

```json
{
  "cols": 80,
  "rows": 24,
  "cursor": { "row": 5, "col": 12, "visible": true },
  "alt_screen_active": true,
  "plain_text": "...",
  "cells": [
    { "row": 0, "col": 0, "ch": "H", "fg": "default", "bg": "default", "bold": true }
  ]
}
```

---

## Integration test scenarios

### A. Simple command output

Baseline. Confirms `run` works end-to-end.

```bash
rterm-cli create e2e-simple
rterm-cli run e2e-simple "echo hello-world"
# assert output contains "hello-world"
rterm-cli kill e2e-simple
```

---

### B. Multi-command sequence

Confirms session state persists between calls.

```bash
rterm-cli create e2e-state
rterm-cli run e2e-state "export FOO=bar"
rterm-cli run e2e-state "echo \$FOO"
# assert output contains "bar"
rterm-cli kill e2e-state
```

---

### C. Vim: open, edit, save, quit

Tests alt-screen handling, insert mode, and `:wq`.

```bash
rterm-cli create e2e-vim --cols 80 --rows 24
rterm-cli exec   e2e-vim "vim /tmp/rterm-test.txt"
rterm-cli wait   e2e-vim "~"              # vim opened (empty buffer = tildes)

# Enter insert mode and type
rterm-cli press  e2e-vim i               # i → INSERT mode
rterm-cli wait   e2e-vim "INSERT"        # status bar shows INSERT
rterm-cli type   e2e-vim "hello from rterm automation"
rterm-cli press  e2e-vim Escape          # back to normal mode
rterm-cli assert e2e-vim "INSERT"        # INSERT gone from status bar

# Save and quit
rterm-cli type   e2e-vim ":wq"
rterm-cli press  e2e-vim Enter
rterm-cli wait   e2e-vim "\$"            # back to shell prompt

# Verify file was written
rterm-cli run    e2e-vim "cat /tmp/rterm-test.txt"
# assert output contains "hello from rterm automation"
rterm-cli kill   e2e-vim
```

---

### D. Vim: navigation and search

Tests VT cursor movement and vim's `/ ` search.

```bash
rterm-cli create e2e-vimnav --cols 80 --rows 24
rterm-cli run    e2e-vimnav "echo -e 'line1\nline2\nline3' > /tmp/nav.txt"
rterm-cli exec   e2e-vimnav "vim /tmp/nav.txt"
rterm-cli wait   e2e-vimnav "line1"

# Search for "line3"
rterm-cli type   e2e-vimnav "/line3"
rterm-cli press  e2e-vimnav Enter
rterm-cli cursor e2e-vimnav
# assert cursor row = 2 (0-indexed: "line3" is on row 2)

rterm-cli press  e2e-vimnav Escape
rterm-cli type   e2e-vimnav ":q!"
rterm-cli press  e2e-vimnav Enter
rterm-cli kill   e2e-vimnav
```

---

### E. Python REPL

Tests REPL interaction: send expression, wait for result, send `exit()`.

```bash
rterm-cli create e2e-py --cols 80 --rows 24
rterm-cli exec   e2e-py "python3"
rterm-cli wait   e2e-py ">>>"               # Python prompt

rterm-cli type   e2e-py "2 + 2\n"
rterm-cli wait   e2e-py "4"                 # result on screen
rterm-cli assert e2e-py ">>>"              # prompt returned

rterm-cli type   e2e-py "exit()\n"
rterm-cli wait   e2e-py "\$"               # back to shell
rterm-cli kill   e2e-py
```

---

### F. Ctrl+C interrupts a running process

Tests SIGINT delivery through the PTY.

```bash
rterm-cli create e2e-sigint
rterm-cli exec   e2e-sigint "sleep 60"
# sleep is running, no prompt

rterm-cli press  e2e-sigint Ctrl+C
rterm-cli wait   e2e-sigint "\$"           # shell prompt returned
rterm-cli assert e2e-sigint "sleep 60"    # command line still visible
rterm-cli kill   e2e-sigint
```

---

### G. Resize mid-session

Tests that resize is reflected in subsequent snapshots and VT layout.

```bash
rterm-cli create  e2e-resize --cols 80 --rows 24
rterm-cli run     e2e-resize "echo hi"
rterm-cli resize  e2e-resize --cols 120 --rows 40
rterm-cli snapshot-json e2e-resize
# assert cols=120, rows=40
rterm-cli kill    e2e-resize
```

---

### H. `WaitForText` timeout path

Confirms the server returns promptly with `found=false` when the pattern
never appears.

```bash
rterm-cli create e2e-timeout
# Don't send anything that would print "XYZZY"
rterm-cli wait   e2e-timeout "XYZZY" --timeout-ms 300
# expect: exit code 1 (not found), elapsed ≈ 300ms
rterm-cli kill   e2e-timeout
```

---

---

## Integration test scenarios (streaming exec)

### I. Streaming exec output

Baseline. Confirms `exec` streams output in real-time and returns correct exit code.

```bash
rterm-cli exec --cwd /tmp -- echo hello
# assert output contains "hello"
# assert exit_code == 0

rterm-cli exec --cwd /nonexistent -- ls
# assert exit_code != 0 (non-zero exit)
```

### J. Streaming exec timeout

Confirms timeout kills the process and returns correct exit code.

```bash
rterm-cli exec --cwd /tmp -- sleep 10 --timeout-ms 500
# assert timed_out == true
# assert exit_code == 124  (standard timeout exit code)
```

### K. Streaming exec with large output

Confirms output is streamed progressively (not buffered until end).

```bash
rterm-cli exec --cwd /tmp -- yes | head -n 10000
# assert output streams progressively, not all at once at end
# assert exit_code == 0
```

---

## Test implementation plan

### Unit tests (no network, no PTY)
- **`rterm-proto`**: round-trip encode/decode for all 6 new automation types
- **`rterm-relay/managed_session`**: `plain_text()` correctness; `resize()` updates `cols`/`rows`

### In-process integration tests (FakePtySpawner, no network)
- All service handlers: success paths + error paths
- `WaitForText`: found path (inject PTY output, confirm `found=true`)
- `WaitForText`: timeout path (no output, confirm `found=false` within ~timeout)
- `RunCommand`: sentinel detected, output trimmed
- `RunCommand`: timeout path

### Docker E2E tests (full stack)
- Scenarios A–D above (vim requires the container to have vim installed)
- Scenario F (Ctrl+C)
- Scenario H (timeout)

Python REPL and resize tests are lower priority for Docker E2E — cover them
in-process first.

---

## Resolved design decisions

1. **`press` + application cursor key mode** — `PressKeys` is a dedicated RPC.
   The server reads `terminal.modes.application_cursor_keys` from the live
   session and resolves `Up`/`Down`/`Left`/`Right` to `\x1bOA`/`\x1bOB`/`\x1bOC`/`\x1bOD`
   (application mode) or `\x1b[A`/`\x1b[B`/`\x1b[C`/`\x1b[D` (normal mode) accordingly.

2. **`assert` vs `wait --timeout-ms 0`** — Implemented as a CLI-only command
   that calls `WaitForText` with `timeout_ms=0` (server checks once and returns).
   No new RPC needed.

3. **Sentinel collision** — Fixed: `RunCommand` uses a global `AtomicU64` counter
   (`RUN_CMD_COUNTER`) so every call gets a unique sentinel regardless of timing.

4. **Output capture boundaries** — Fixed: `RunCommandSvc` snapshots the screen
   before sending the command (`text_before`), then after the sentinel appears,
   filters out lines that existed in `text_before`, the sentinel line, and the
   echoed command line. Result is only the new output lines.

5. **Exec streaming: merged stdout/stderr** — PTY naturally merges stdout and stderr
   into a single stream. All output arrives via `stdout`. The `stderr` field in
   `ExecResponse` is always empty. If true stderr separation is needed, the client
   can redirect in the command itself (e.g., `bash -c "cmd 2>/tmp/stderr"`).

6. **Exec streaming: raw bytes** — Output is streamed as raw `Vec<u8>` bytes,
   not decoded strings. This gives clients flexibility to handle UTF-8 text,
   binary output, or anything else.
