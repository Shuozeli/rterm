use rterm_core::Terminal;

/// Test that demonstrates what Ink/Claude Code's output looks like
/// in our emulator. Check for line offset issues.
#[test]
fn ink_box_drawing_alignment() {
    let mut t = Terminal::new(80, 24);
    // Simulate Claude Code's welcome box
    t.feed(b"\x1b[38;5;174m\xe2\x95\xad\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\x1b[1CClaude\x1b[1CCode\x1b[1Cv2.1.87\x1b[1C\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x95\xae\x1b[39m\r\n");
    t.feed(b"\x1b[38;5;174m\xe2\x94\x82\x1b[39m Welcome \x1b[38;5;174m\xe2\x94\x82\x1b[39m\r\n");
    t.feed(b"\x1b[38;5;174m\xe2\x95\xb0\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\xe2\x95\xaf\x1b[39m\r\n");
    
    // Check that rows are at correct positions
    println!("Row 0: |{}|", t.screen().row_text(0));
    println!("Row 1: |{}|", t.screen().row_text(1));
    println!("Row 2: |{}|", t.screen().row_text(2));
    println!("Row 3: |{}|", t.screen().row_text(3));
    
    // Row 0 should start with ╭
    assert_eq!(t.screen().cell(0, 0).ch, '╭');
    // Row 1 should start with │
    assert_eq!(t.screen().cell(1, 0).ch, '│');
    // Row 2 should start with ╰
    assert_eq!(t.screen().cell(2, 0).ch, '╰');
    // Row 3 should be empty
    assert_eq!(t.screen().cell(3, 0).ch, ' ');
    
    // Cursor should be on row 3
    assert_eq!(t.screen().cursor.row, 3);
}

/// Test CSI C (cursor forward) spacing — Ink uses this instead of spaces
#[test]
fn cursor_forward_spacing() {
    let mut t = Terminal::new(40, 5);
    // Ink pattern: text, cursor forward, text
    t.feed(b"A\x1b[3CB\x1b[5CC");
    
    println!("Row 0: |{}|", t.screen().row_text(0));
    // A at col 0, B at col 4, C at col 10
    assert_eq!(t.screen().cell(0, 0).ch, 'A');
    assert_eq!(t.screen().cell(0, 1).ch, ' '); // gap
    assert_eq!(t.screen().cell(0, 4).ch, 'B');
    assert_eq!(t.screen().cell(0, 5).ch, ' '); // gap
    assert_eq!(t.screen().cell(0, 10).ch, 'C');
}

/// Test the actual Claude Code welcome box from captured output
#[test] 
fn claude_code_welcome_box() {
    let data = match std::fs::read("/tmp/claude-raw.bin") {
        Ok(d) => d,
        Err(_) => { eprintln!("Skipping: /tmp/claude-raw.bin not found"); return; }
    };
    let mut t = Terminal::new(80, 24);
    t.feed(&data);
    
    // Dump all non-empty rows with their positions
    for row in 0..24 {
        let text = t.screen().row_text(row);
        if !text.is_empty() {
            // Also show the first few cell chars to check alignment
            let cells: String = (0..text.len().min(5))
                .map(|c| format!("{}:{} ", c, t.screen().cell(row, c).ch))
                .collect();
            println!("Row {:2}: cells=[{}] text=|{}|", row, cells.trim(), text);
        }
    }
    
    // The welcome box should have ╭ at row 0 col 0
    let first_char = t.screen().cell(0, 0).ch;
    println!("\nFirst char: '{}' (U+{:04X})", first_char, first_char as u32);
    
    // Check specific alignment expectations
    // Row 0: ╭─── Claude Code ...
    assert_eq!(first_char, '╭', "first char should be ╭");
    
    // The bottom of the box
    assert_eq!(t.screen().cell(10, 0).ch, '╰', "row 10 should have ╰");
}

/// Test that \r\n doesn't cause double line feeds
#[test]
fn crlf_no_double_feed() {
    let mut t = Terminal::new(80, 5);
    t.feed(b"Line1\r\nLine2\r\nLine3\r\n");
    
    assert_eq!(t.screen().row_text(0), "Line1");
    assert_eq!(t.screen().row_text(1), "Line2");
    assert_eq!(t.screen().row_text(2), "Line3");
    assert_eq!(t.screen().row_text(3), ""); // should be empty, not another line
    assert_eq!(t.screen().cursor.row, 3);
}

/// Test cursor up followed by overwrite — Ink's re-render pattern
#[test]
fn cursor_up_overwrite() {
    let mut t = Terminal::new(80, 10);
    // Write 3 lines
    t.feed(b"AAA\r\nBBB\r\nCCC");
    assert_eq!(t.screen().cursor.row, 2);
    
    // Go up 2 and overwrite
    t.feed(b"\x1b[2A");
    assert_eq!(t.screen().cursor.row, 0);
    
    t.feed(b"\r\x1b[0KXXX"); // CR + clear line + write
    assert_eq!(t.screen().row_text(0), "XXX");
    assert_eq!(t.screen().row_text(1), "BBB"); // unchanged
    assert_eq!(t.screen().row_text(2), "CCC"); // unchanged
}
