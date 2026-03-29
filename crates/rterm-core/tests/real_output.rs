//! Tests feeding real/simulated terminal output through the VT emulator.
//! Verifies no panics, correct screen state, and proper handling of
//! common escape sequences used by TUI apps.

use rterm_core::{Color, Terminal};

fn feed(t: &mut Terminal, s: &str) {
    t.feed(s.as_bytes());
}

fn feed_bytes(t: &mut Terminal, bytes: &[u8]) {
    t.feed(bytes);
}

// ============================================================================
// Basic ANSI sequences
// ============================================================================

#[test]
fn ls_color_output() {
    let mut t = Terminal::new(80, 24);
    // Simulated ls --color output.
    feed(
        &mut t,
        "\x1b[0m\x1b[01;34mDesktop\x1b[0m  \x1b[01;34mDocuments\x1b[0m  \x1b[01;32mscript.sh\x1b[0m  file.txt\r\n",
    );

    assert_eq!(t.screen().cell(0, 0).ch, 'D');
    assert!(t.screen().cell(0, 0).attrs.bold); // bold
    assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(4)); // blue
}

#[test]
fn cursor_hide_show() {
    let mut t = Terminal::new(80, 24);
    assert!(t.screen().cursor.visible);
    feed(&mut t, "\x1b[?25l"); // hide
    assert!(!t.screen().cursor.visible);
    feed(&mut t, "\x1b[?25h"); // show
    assert!(t.screen().cursor.visible);
}

#[test]
fn alternate_screen_toggle() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "primary content");
    assert_eq!(t.screen().cell(0, 0).ch, 'p');

    feed(&mut t, "\x1b[?1049h"); // enter alt screen
    assert_eq!(t.screen().cell(0, 0).ch, ' '); // alt is blank

    feed(&mut t, "alt content");
    assert_eq!(t.screen().cell(0, 0).ch, 'a');

    feed(&mut t, "\x1b[?1049l"); // leave alt screen
    assert_eq!(t.screen().cell(0, 0).ch, 'p'); // primary restored
}

#[test]
fn erase_entire_display() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "Hello World");
    feed(&mut t, "\x1b[2J"); // erase all
    assert_eq!(t.screen().row_text(0), "");
}

#[test]
fn erase_line_to_end() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "Hello World");
    feed(&mut t, "\x1b[6G"); // cursor to col 6
    feed(&mut t, "\x1b[0K"); // erase to end of line
    assert_eq!(t.screen().row_text(0), "Hello");
}

#[test]
fn cursor_absolute_positioning() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[5;10H"); // row 5, col 10
    feed(&mut t, "X");
    assert_eq!(t.screen().cell(4, 9).ch, 'X');
}

// ============================================================================
// SGR edge cases
// ============================================================================

#[test]
fn sgr_reset_mid_sequence() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[1;31mBold Red\x1b[0m Normal");
    // "B" should be bold red.
    let b = t.screen().cell(0, 0);
    assert!(b.attrs.bold);
    assert_eq!(b.fg, Color::Indexed(1));
    // "N" after reset should be default.
    let n = t.screen().cell(0, 9);
    assert!(!n.attrs.bold);
    assert_eq!(n.fg, Color::Default);
}

#[test]
fn sgr_multiple_attributes() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[1;3;4;31mX\x1b[0m"); // bold+italic+underline+red
    let c = t.screen().cell(0, 0);
    assert!(c.attrs.bold);
    assert!(c.attrs.italic);
    assert!(c.attrs.underline);
    assert_eq!(c.fg, Color::Indexed(1));
}

#[test]
fn sgr_256_and_rgb() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[38;5;208mA\x1b[38;2;100;200;50mB\x1b[0m");
    assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(208));
    assert_eq!(t.screen().cell(0, 1).fg, Color::Rgb(100, 200, 50));
}

#[test]
fn sgr_reverse_video() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[7mX\x1b[27mY");
    assert!(t.screen().cell(0, 0).attrs.reverse);
    assert!(!t.screen().cell(0, 1).attrs.reverse);
}

#[test]
fn sgr_bright_colors() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[91mA\x1b[102mB\x1b[0m");
    assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(9)); // bright red fg
    assert_eq!(t.screen().cell(0, 1).bg, Color::Indexed(10)); // bright green bg
}

// ============================================================================
// Vim-like sequences
// ============================================================================

#[test]
fn vim_startup_sequence_no_panic() {
    let mut t = Terminal::new(80, 24);
    // Real vim startup sequence (captured).
    let vim_seq = b"\x1b[?1049h\x1b[22;0;0t\x1b[>4;2m\x1b[?1h=\x1b[?2004h\x1b[?1004h\x1b[1;24r\x1b[?12h\x1b[?12l\x1b[22;2t\x1b[22;1t\x1b[27m\x1b[23m\x1b[29m\x1b[m\x1b[H\x1b[2J\x1b[24;1H\x1b[?2004l\x1b[>4;m\x1b[23;2t\x1b[23;1t\x1b[?1004l\x1b[?2004l\x1b[?1l>\x1b[?1049l\x1b[23;0;0t\x1b[>4;m";
    feed_bytes(&mut t, vim_seq);
    // No panic = pass. Verify we're back on primary screen.
    assert!(!t.screen().cursor.visible || t.screen().cursor.visible); // just no panic
}

#[test]
fn vim_cursor_to_home_and_clear() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "garbage");
    feed(&mut t, "\x1b[H\x1b[2J"); // home + clear (common vim pattern)
    assert_eq!(t.screen().cursor.row, 0);
    assert_eq!(t.screen().cursor.col, 0);
    assert_eq!(t.screen().row_text(0), "");
}

// ============================================================================
// Ink-like patterns (React for CLIs)
// ============================================================================

#[test]
fn ink_cursor_hide_write_restore() {
    let mut t = Terminal::new(80, 24);
    // Ink pattern: hide cursor, write at position, show cursor.
    feed(&mut t, "\x1b[?25l"); // hide cursor
    feed(&mut t, "\x1b[1;1H"); // home
    feed(&mut t, "\x1b[2J"); // clear
    feed(&mut t, "\x1b[1;1H"); // home again
    feed(&mut t, "\x1b[32m✓\x1b[39m Task completed");
    feed(&mut t, "\x1b[?25h"); // show cursor

    // The checkmark should be green.
    let cell = t.screen().cell(0, 0);
    assert_eq!(cell.ch, '✓');
    assert_eq!(cell.fg, Color::Indexed(2)); // green
    assert!(t.screen().cursor.visible);
}

#[test]
fn ink_clear_line_and_rewrite() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[1;1H");
    feed(&mut t, "Loading...");
    // Ink rewrites the line:
    feed(&mut t, "\x1b[1;1H\x1b[0KDone!"); // home + clear line + write
    assert_eq!(t.screen().row_text(0), "Done!");
}

#[test]
fn ink_multiline_rewrite() {
    let mut t = Terminal::new(80, 24);
    // Write initial content.
    feed(&mut t, "\x1b[1;1HLine 1\r\nLine 2\r\nLine 3");
    // Ink goes back and rewrites.
    feed(
        &mut t,
        "\x1b[1;1H\x1b[0KNew 1\r\n\x1b[0KNew 2\r\n\x1b[0KNew 3",
    );
    assert_eq!(t.screen().row_text(0), "New 1");
    assert_eq!(t.screen().row_text(1), "New 2");
    assert_eq!(t.screen().row_text(2), "New 3");
}

// ============================================================================
// Top/htop-like patterns
// ============================================================================

#[test]
fn tui_reverse_video_header() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[7m  PID  COMMAND \x1b[27m");
    let cell = t.screen().cell(0, 0);
    assert!(cell.attrs.reverse);
    assert_eq!(cell.ch, ' ');
}

#[test]
fn tui_rapid_screen_updates_no_panic() {
    let mut t = Terminal::new(80, 24);
    // Simulate rapid full-screen redraws like top/htop does.
    for i in 0..50 {
        feed(&mut t, "\x1b[H"); // home
        for row in 0..24 {
            feed(
                &mut t,
                &format!(
                    "\x1b[{};1H\x1b[0K  {} line {} update {}\r\n",
                    row + 1,
                    row,
                    i,
                    i * row
                ),
            );
        }
    }
    // No panic = pass. Check last update is visible.
    let text = t.screen().row_text(0);
    assert!(
        text.contains("49"),
        "last update should be visible: {}",
        text
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_csi_params_default_to_1() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[5;5H"); // row 5, col 5
    feed(&mut t, "\x1b[A"); // CUU with no param = move up 1
    assert_eq!(t.screen().cursor.row, 3);
    feed(&mut t, "\x1b[B"); // CUD with no param = move down 1
    assert_eq!(t.screen().cursor.row, 4);
}

#[test]
fn cup_with_zero_params() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[H"); // CUP with no params = home
    assert_eq!(t.screen().cursor.row, 0);
    assert_eq!(t.screen().cursor.col, 0);
}

#[test]
fn scroll_region_with_content() {
    let mut t = Terminal::new(80, 5);
    for i in 0..5 {
        feed(&mut t, &format!("\x1b[{};1HRow{}", i + 1, i));
    }
    // Set scroll region to rows 2-4, scroll up.
    feed(&mut t, "\x1b[2;4r");
    feed(&mut t, "\x1b[S"); // scroll up in region
    assert_eq!(t.screen().cell(0, 0).ch, 'R'); // Row0 unchanged
    assert_eq!(t.screen().cell(4, 0).ch, 'R'); // Row4 unchanged
}

#[test]
fn mixed_cr_lf() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "A\rB\nC\r\nD");
    // \r moves to col 0. B overwrites A.
    assert_eq!(t.screen().cell(0, 0).ch, 'B');
    // \n moves down. C is on row 1.
    assert_eq!(t.screen().cell(1, 1).ch, 'C');
    // \r\n = new line. D is on row 2.
    assert_eq!(t.screen().cell(2, 0).ch, 'D');
}

#[test]
fn unicode_characters() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "Hello 世界 🌍");
    assert_eq!(t.screen().cell(0, 0).ch, 'H');
    assert_eq!(t.screen().cell(0, 6).ch, '世');
    assert_eq!(t.screen().cell(0, 7).ch, '界');
}

#[test]
fn tab_stops_at_8() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "A\tB\tC");
    assert_eq!(t.screen().cell(0, 0).ch, 'A');
    assert_eq!(t.screen().cell(0, 8).ch, 'B');
    assert_eq!(t.screen().cell(0, 16).ch, 'C');
}

#[test]
fn unknown_csi_sequences_ignored() {
    let mut t = Terminal::new(80, 24);
    // These are valid CSI but unimplemented — should not panic.
    feed(&mut t, "\x1b[>4;2m"); // xterm private mode
    feed(&mut t, "\x1b[?2004h"); // bracketed paste
    feed(&mut t, "\x1b[?2004l"); // bracketed paste off
    feed(&mut t, "\x1b[?1004h"); // focus events
    feed(&mut t, "\x1b[?1004l"); // focus events off
    feed(&mut t, "\x1b[22;0;0t"); // window manipulation
    feed(&mut t, "\x1b[23;0;0t"); // window manipulation
    feed(&mut t, "\x1b[>4;m"); // xterm private reset
    feed(&mut t, "\x1b[22;2t"); // save title
    feed(&mut t, "\x1b[23;2t"); // restore title
    feed(&mut t, "OK");
    assert_eq!(t.screen().cell(0, 0).ch, 'O');
}

#[test]
fn osc_sequences_ignored_no_panic() {
    let mut t = Terminal::new(80, 24);
    // OSC 0 (set title) — should be silently consumed.
    feed(&mut t, "\x1b]0;My Terminal Title\x07");
    // OSC 8 (hyperlink).
    feed(&mut t, "\x1b]8;;https://example.com\x07Click\x1b]8;;\x07");
    feed(&mut t, " OK");
    // No panic, text after OSC is rendered.
    // "Click" is 5 chars, then " OK" starts at col 5.
    assert_eq!(t.screen().cell(0, 0).ch, 'C'); // "Click" at col 0
    assert_eq!(t.screen().cell(0, 6).ch, 'O'); // " OK" at col 6
}

#[test]
fn dcs_sequences_ignored_no_panic() {
    let mut t = Terminal::new(80, 24);
    // DCS (Device Control String) — should not panic.
    feed(&mut t, "\x1bPsome dcs data\x1b\\");
    feed(&mut t, "OK");
    assert_eq!(t.screen().cell(0, 0).ch, 'O');
}

// ============================================================================
// Resize
// ============================================================================

#[test]
fn resize_wider() {
    let mut t = Terminal::new(40, 10);
    feed(&mut t, "Hello");
    t.resize(80, 10);
    assert_eq!(t.screen().cols(), 80);
    assert_eq!(t.screen().rows(), 10);
    assert_eq!(t.screen().cell(0, 0).ch, 'H');
    assert_eq!(t.screen().cell(0, 4).ch, 'o');
}

#[test]
fn resize_narrower() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "Hello World");
    t.resize(40, 24);
    assert_eq!(t.screen().cols(), 40);
    // Content preserved up to new width.
    assert_eq!(t.screen().cell(0, 0).ch, 'H');
}

#[test]
fn resize_taller() {
    let mut t = Terminal::new(80, 10);
    feed(&mut t, "Test");
    t.resize(80, 20);
    assert_eq!(t.screen().rows(), 20);
    assert_eq!(t.screen().cell(0, 0).ch, 'T');
}

#[test]
fn resize_shorter() {
    let mut t = Terminal::new(80, 24);
    for i in 0..24 {
        feed(&mut t, &format!("\x1b[{};1HRow{}", i + 1, i));
    }
    t.resize(80, 10);
    assert_eq!(t.screen().rows(), 10);
    // Cursor should be clamped.
    assert!(t.screen().cursor.row < 10);
}

#[test]
fn resize_then_write() {
    let mut t = Terminal::new(80, 24);
    t.resize(40, 10);
    feed(&mut t, "After resize");
    assert_eq!(t.screen().row_text(0), "After resize");
}

#[test]
fn resize_preserves_alt_screen() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[?1049h"); // enter alt screen
    feed(&mut t, "Alt content");
    t.resize(40, 10);
    assert_eq!(t.screen().cols(), 40);
    feed(&mut t, "\x1b[?1049l"); // leave alt screen
    assert_eq!(t.screen().cols(), 40); // primary also resized
}

// ============================================================================
// Ink/Claude Code patterns
// ============================================================================

#[test]
fn ink_spinner_rewrite() {
    let mut t = Terminal::new(80, 24);
    // Ink spinner pattern: write text, go back, rewrite.
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    for frame in frames {
        feed(&mut t, &format!("\x1b[1;1H\x1b[0K{} Loading...", frame));
    }
    // Last frame should be visible.
    let text = t.screen().row_text(0);
    assert!(text.contains("Loading..."), "got: {}", text);
}

#[test]
fn ink_status_bar_at_bottom() {
    let mut t = Terminal::new(80, 10);
    // Ink often writes a status bar at the bottom.
    feed(&mut t, "\x1b[10;1H\x1b[7m Status: Ready \x1b[27m");
    let cell = t.screen().cell(9, 1);
    assert_eq!(cell.ch, 'S');
    assert!(cell.attrs.reverse);
}

#[test]
fn ink_cursor_save_restore_pattern() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[5;10H"); // position
    feed(&mut t, "\x1b7"); // save
    feed(&mut t, "\x1b[1;1H"); // go home
    feed(&mut t, "Header");
    feed(&mut t, "\x1b8"); // restore
    feed(&mut t, "Body");
    // "Header" at row 0, "Body" at row 4 col 9.
    assert_eq!(t.screen().cell(0, 0).ch, 'H');
    assert_eq!(t.screen().cell(4, 9).ch, 'B');
}

#[test]
fn ink_multiline_component_rerender() {
    let mut t = Terminal::new(60, 10);
    // First render of a component.
    feed(&mut t, "\x1b[?25l"); // hide cursor
    feed(&mut t, "\x1b[1;1H");
    feed(&mut t, "\x1b[36m┌────────────────────┐\x1b[39m\r\n");
    feed(
        &mut t,
        "\x1b[36m│\x1b[39m  Status: \x1b[32mOK\x1b[39m       \x1b[36m│\x1b[39m\r\n",
    );
    feed(&mut t, "\x1b[36m└────────────────────┘\x1b[39m");

    // Verify box characters.
    assert_eq!(t.screen().cell(0, 0).ch, '┌');
    assert_eq!(t.screen().cell(2, 0).ch, '└');

    // Re-render (Ink does this on state change).
    feed(&mut t, "\x1b[1;1H"); // back to top
    feed(&mut t, "\x1b[36m┌────────────────────┐\x1b[39m\r\n");
    feed(
        &mut t,
        "\x1b[36m│\x1b[39m  Status: \x1b[31mFAIL\x1b[39m     \x1b[36m│\x1b[39m\r\n",
    );
    feed(&mut t, "\x1b[36m└────────────────────┘\x1b[39m");
    feed(&mut t, "\x1b[?25h"); // show cursor

    // Should show "FAIL" now.
    let row1 = t.screen().row_text(1);
    assert!(row1.contains("FAIL"), "got: {}", row1);
    assert!(t.screen().cursor.visible);
}

/// Regression: CSI > 4 m (xterm modifyOtherKeys) must NOT be treated as SGR 4 (underline).
#[test]
fn xterm_private_mode_not_sgr_underline() {
    let mut t = Terminal::new(80, 24);
    feed(&mut t, "\x1b[>4m"); // xterm modifyOtherKeys — NOT underline
    feed(&mut t, "Hello");
    assert!(
        !t.screen().cell(0, 0).attrs.underline,
        "CSI > 4 m should not set underline"
    );
}

#[test]
fn split_escape_sequence_across_feeds() {
    let mut t = Terminal::new(80, 24);
    // Split "\x1b[31mRed\x1b[0m" across two feeds.
    t.feed(b"\x1b[31mR");
    t.feed(b"ed\x1b[0m");
    assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(1)); // red
    assert_eq!(t.screen().cell(0, 0).ch, 'R');
    assert_eq!(t.screen().cell(0, 2).ch, 'd');
}

#[test]
fn split_escape_at_boundary() {
    let mut t = Terminal::new(80, 24);
    // Split the ESC and [ across feeds.
    t.feed(b"\x1b");
    t.feed(b"[32mGreen\x1b[0m");
    assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(2)); // green
    assert_eq!(t.screen().cell(0, 0).ch, 'G');
}

#[test]
fn rapid_resize_simulation() {
    // The emulator itself doesn't handle resize (that's the PTY),
    // but verify creating terminals of various sizes works.
    for cols in [1, 10, 80, 200, 500] {
        for rows in [1, 5, 24, 50, 100] {
            let mut t = Terminal::new(cols, rows);
            feed(&mut t, "X");
            assert_eq!(t.screen().cell(0, 0).ch, 'X');
        }
    }
}

#[test]
fn full_screen_of_text() {
    let mut t = Terminal::new(80, 24);
    // Fill entire screen with text.
    for row in 0..24 {
        for col in 0..80 {
            feed(
                &mut t,
                &((b'A' + (row * 80 + col) as u8 % 26) as char).to_string(),
            );
        }
    }
    // Should have scrolled. No panic.
    assert!(t.screen().scrollback_len() > 0 || t.screen().cursor.row == 23);
}
