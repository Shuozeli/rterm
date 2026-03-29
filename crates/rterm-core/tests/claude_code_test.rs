use rterm_core::Terminal;

#[test]
fn claude_code_output_no_panic() {
    let data = match std::fs::read("/tmp/claude-raw.bin") {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Skipping: /tmp/claude-raw.bin not found");
            return;
        }
    };
    let mut t = Terminal::new(80, 24);
    t.feed(&data);
    
    // Should not panic. Check screen has some content.
    let mut has_content = false;
    for row in 0..t.screen().rows() {
        let text = t.screen().row_text(row);
        if !text.is_empty() {
            has_content = true;
            println!("Row {:2}: |{}|", row, text);
        }
    }
    println!("\nCursor: row={}, col={}", t.screen().cursor.row, t.screen().cursor.col);
    assert!(has_content, "screen should have some content after Claude Code output");
}
