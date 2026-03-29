/// Detailed rendering test: feed real Claude Code output through the emulator
/// and dump cell-by-cell state to find rendering issues.
use rterm_core::{Color, Terminal};

#[test]
fn claude_code_detailed_render() {
    let data = match std::fs::read("/tmp/claude-pty.bin") {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Skipping: /tmp/claude-pty.bin not found");
            return;
        }
    };

    let mut t = Terminal::new(80, 24);

    // Feed in chunks to simulate network delivery
    let chunk_size = 256;
    for chunk in data.chunks(chunk_size) {
        t.feed(chunk);
    }

    println!("\n=== FINAL SCREEN STATE (80x24) ===\n");

    for row in 0..24 {
        let mut line = String::new();
        let mut has_underline = false;
        let mut has_dim = false;

        for col in 0..80 {
            let cell = t.screen().cell(row, col);
            if cell.attrs.underline {
                has_underline = true;
            }
            if cell.attrs.dim {
                has_dim = true;
            }
            line.push(cell.ch);
        }

        let trimmed = line.trim_end().to_string();
        if !trimmed.is_empty() || has_underline || has_dim {
            let mut flags = Vec::new();
            if has_underline {
                flags.push("UL");
            }
            if has_dim {
                flags.push("DIM");
            }
            let flag_str = if flags.is_empty() {
                "  ".to_string()
            } else {
                flags.join(",")
            };

            let details: Vec<String> = (0..80)
                .filter(|&c| {
                    let cell = t.screen().cell(row, c);
                    cell.ch != ' ' || cell.attrs.underline || cell.bg != Color::Default
                })
                .take(5)
                .map(|c| {
                    let cell = t.screen().cell(row, c);
                    let mut attrs = Vec::new();
                    if cell.attrs.bold {
                        attrs.push("B");
                    }
                    if cell.attrs.dim {
                        attrs.push("D");
                    }
                    if cell.attrs.underline {
                        attrs.push("U");
                    }
                    if cell.attrs.reverse {
                        attrs.push("R");
                    }
                    if cell.bg != Color::Default {
                        attrs.push("BG");
                    }
                    format!(
                        "{}:'{}'{}",
                        c,
                        cell.ch,
                        if attrs.is_empty() {
                            String::new()
                        } else {
                            format!("({})", attrs.join(","))
                        }
                    )
                })
                .collect();

            println!(
                "Row {:2} [{}]: |{}|  cells: [{}]",
                row,
                flag_str,
                trimmed,
                details.join(", ")
            );
        }
    }

    println!(
        "\nCursor: row={}, col={}, visible={}",
        t.screen().cursor.row,
        t.screen().cursor.col,
        t.screen().cursor.visible
    );

    // Key assertions
    assert_eq!(t.screen().cell(0, 0).ch, '╭', "Row 0 should start with ╭");

    // Check for unexpected underlines (the "lines under text" issue)
    let mut underline_rows = Vec::new();
    for row in 0..24 {
        for col in 0..80 {
            if t.screen().cell(row, col).attrs.underline {
                underline_rows.push(row);
                break;
            }
        }
    }
    if !underline_rows.is_empty() {
        println!(
            "\nWARNING: Rows with underline attribute: {:?}",
            underline_rows
        );
        for &row in &underline_rows {
            let cells: Vec<String> = (0..80)
                .filter(|&c| t.screen().cell(row, c).attrs.underline)
                .map(|c| format!("col{}:'{}'", c, t.screen().cell(row, c).ch))
                .collect();
            println!("  Row {}: underlined cells: [{}]", row, cells.join(", "));
        }
    }

    // Check for unexpected dim text
    let mut dim_rows = Vec::new();
    for row in 0..24 {
        for col in 0..80 {
            if t.screen().cell(row, col).attrs.dim && t.screen().cell(row, col).ch != ' ' {
                dim_rows.push(row);
                break;
            }
        }
    }
    if !dim_rows.is_empty() {
        println!("\nRows with dim text: {:?}", dim_rows);
    }

    // Check for background colors that might cause visual issues
    let mut bg_rows = Vec::new();
    for row in 0..24 {
        for col in 0..80 {
            if t.screen().cell(row, col).bg != Color::Default {
                bg_rows.push(row);
                break;
            }
        }
    }
    if !bg_rows.is_empty() {
        println!("\nRows with non-default background: {:?}", bg_rows);
        for &row in &bg_rows {
            let cells: Vec<String> = (0..80)
                .filter(|&c| t.screen().cell(row, c).bg != Color::Default)
                .take(10)
                .map(|c| format!("col{}:bg={:?}", c, t.screen().cell(row, c).bg))
                .collect();
            println!("  Row {}: [{}]", row, cells.join(", "));
        }
    }
}
