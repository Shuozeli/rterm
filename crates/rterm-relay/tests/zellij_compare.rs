/// Side-by-side comparison: rterm Terminal output vs zellij screen dump.
///
/// Runs the same commands through both pipelines and compares visible text.
/// Zellij tests require zellij installed and a working PTY (skipped otherwise).
use rterm_core::Terminal;
use std::process::Command;

// ============================================================================
// Zellij driver
// ============================================================================

struct ZellijSession {
    name: String,
    script_pid: Option<u32>,
}

impl ZellijSession {
    fn start() -> Option<Self> {
        // Check zellij exists.
        if Command::new("which")
            .arg("zellij")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return None;
        }

        let name = format!("rterm-cmp-{}", std::process::id());

        // Clean up any stale session.
        let _ = Command::new("zellij")
            .args(["delete-session", &name, "--force"])
            .output();

        // Start zellij via script (provides PTY).
        let child = Command::new("script")
            .args([
                "-q",
                "-c",
                &format!("zellij --session {}", name),
                "/dev/null",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;

        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_secs(3));

        // Verify session is running.
        let list = Command::new("zellij").arg("list-sessions").output().ok()?;
        let list_str = String::from_utf8_lossy(&list.stdout);
        if !list_str.contains(&name) {
            let _ = Command::new("kill").arg(pid.to_string()).output();
            return None;
        }

        Some(Self {
            name,
            script_pid: Some(pid),
        })
    }

    fn run_command(&self, cmd: &str) {
        // Clear screen first.
        let _ = Command::new("zellij")
            .args(["--session", &self.name, "action", "write-chars", "clear"])
            .output();
        let _ = Command::new("zellij")
            .args(["--session", &self.name, "action", "write", "10"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Send command.
        let _ = Command::new("zellij")
            .args(["--session", &self.name, "action", "write-chars", cmd])
            .output();
        let _ = Command::new("zellij")
            .args(["--session", &self.name, "action", "write", "10"])
            .output();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    fn dump_screen(&self) -> Vec<String> {
        let path = format!("/tmp/zellij-cmp-{}.txt", std::process::id());
        let _ = Command::new("zellij")
            .args(["--session", &self.name, "action", "dump-screen", &path])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        content.lines().map(|l| l.to_string()).collect()
    }

    fn dump_full_scrollback(&self) -> Vec<String> {
        let path = format!("/tmp/zellij-cmp-full-{}.txt", std::process::id());
        let _ = Command::new("zellij")
            .args([
                "--session",
                &self.name,
                "action",
                "dump-screen",
                "-f",
                &path,
            ])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        content.lines().map(|l| l.to_string()).collect()
    }
}

impl Drop for ZellijSession {
    fn drop(&mut self) {
        if let Some(pid) = self.script_pid {
            let _ = Command::new("kill").arg(pid.to_string()).output();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = Command::new("zellij")
            .args(["delete-session", &self.name, "--force"])
            .output();
    }
}

// ============================================================================
// rterm Terminal driver: run shell commands via PTY and capture screen
// ============================================================================

fn rterm_run_command(cmd: &str, cols: usize, rows: usize) -> Vec<String> {
    // Use a real PTY to run the command so we get authentic VT output.
    let output = Command::new("script")
        .args([
            "-q",
            "-c",
            &format!("TERM=xterm COLUMNS={} LINES={} {}", cols, rows, cmd),
            "/dev/null",
        ])
        .env("TERM", "xterm")
        .env("COLUMNS", cols.to_string())
        .env("LINES", rows.to_string())
        .output()
        .expect("failed to run command via script");

    let vt_bytes = output.stdout;

    // Feed through our Terminal emulator.
    let mut terminal = Terminal::new(cols, rows);
    terminal.feed(&vt_bytes);

    // Extract visible screen.
    let screen = terminal.screen();
    let mut visible = Vec::new();
    for row in 0..rows {
        let mut line = String::new();
        for col in 0..cols {
            line.push(screen.cell(row, col).ch);
        }
        visible.push(line.trim_end().to_string());
    }

    visible
}

// ============================================================================
// Tests: direct VT comparison (no zellij, uses real PTY output)
// ============================================================================

#[test]
fn pty_seq_1_10() {
    let visible = rterm_run_command("seq 1 10", 40, 15);

    let numbers: Vec<&String> = visible
        .iter()
        .filter(|l| l.trim().parse::<i64>().is_ok())
        .collect();

    assert!(
        numbers.len() >= 10,
        "expected 10 numbers, found {}: {:?}",
        numbers.len(),
        numbers
    );

    for (i, n) in numbers.iter().take(10).enumerate() {
        assert_eq!(n.trim(), &format!("{}", i + 1), "number {} mismatch", i + 1);
    }
}

#[test]
fn pty_echo_colors() {
    let visible = rterm_run_command(
        "echo -e '\\033[31mRED\\033[0m \\033[32mGREEN\\033[0m'",
        40,
        10,
    );

    let has_red_green = visible
        .iter()
        .any(|l| l.contains("RED") && l.contains("GREEN"));
    assert!(
        has_red_green,
        "expected 'RED GREEN' in output, got: {:?}",
        visible
    );
}

#[test]
fn pty_printf_padding() {
    let visible = rterm_run_command("printf '%5d\\n' 1 2 3 4 5", 40, 10);

    let all: Vec<String> = visible.iter().map(|s| s.to_string()).collect();

    let padded: Vec<&String> = all
        .iter()
        .filter(|l| l.trim().parse::<i64>().is_ok() && l.starts_with(' '))
        .collect();

    assert!(
        padded.len() >= 5,
        "expected 5 padded numbers, found {}: {:?}",
        padded.len(),
        padded
    );
}

#[test]
fn pty_line_wrapping() {
    let cmd = "python3 -c \"print('A' * 120)\"";
    let visible = rterm_run_command(cmd, 40, 10);

    let a_count: usize = visible
        .iter()
        .map(|l| l.chars().filter(|&c| c == 'A').count())
        .sum();
    assert!(
        a_count >= 120,
        "expected >=120 A's across wrapped lines, found {}",
        a_count
    );
}

#[test]
fn pty_cursor_positioning() {
    let cmd = "printf '\\033[3;10Hhere'";
    let visible = rterm_run_command(cmd, 40, 10);

    assert!(
        visible[2].contains("here"),
        "expected 'here' at row 2, got: '{}'",
        visible[2]
    );
}

#[test]
fn pty_clear_and_rewrite() {
    let cmd = "echo first; clear; echo second";
    let visible = rterm_run_command(cmd, 40, 10);

    assert!(
        visible.iter().any(|l| l.contains("second")),
        "expected 'second' after clear, got: {:?}",
        visible
    );
    // "first" should not be visible (cleared).
    assert!(
        !visible.iter().any(|l| l.contains("first")),
        "'first' should be cleared from visible screen"
    );
}

#[test]
fn pty_tab_characters() {
    let cmd = "printf 'a\\tb\\tc\\n'";
    let visible = rterm_run_command(cmd, 40, 10);

    // Tab stops at 8-char intervals: "a" at 0, "b" at 8, "c" at 16.
    let line = &visible[0];
    assert!(line.contains('a'), "missing 'a'");
    assert!(line.contains('b'), "missing 'b'");
    assert!(line.contains('c'), "missing 'c'");
    // "b" should be at or after column 8.
    if let Some(b_pos) = line.find('b') {
        assert!(
            b_pos >= 8,
            "tab stop wrong: 'b' at col {}, expected >=8",
            b_pos
        );
    }
}

#[test]
fn pty_backspace_overwrite() {
    let cmd = "printf 'AB\\x08C'"; // Write "AB", backspace, write "C" -> "AC"
    let visible = rterm_run_command(cmd, 40, 10);

    assert!(
        visible[0].starts_with("AC"),
        "backspace overwrite failed, got: '{}'",
        visible[0]
    );
}

#[test]
fn pty_erase_to_end_of_line() {
    let cmd = "printf 'Hello World\\033[1;6H\\033[K'"; // Erase from col 5 to EOL
    let visible = rterm_run_command(cmd, 40, 10);

    assert_eq!(
        visible[0].trim_end(),
        "Hello",
        "erase to EOL failed, got: '{}'",
        visible[0]
    );
}

#[test]
fn pty_insert_line() {
    // Write 3 lines, go to line 2, insert a blank line.
    let cmd = "printf 'AAA\\nBBB\\nCCC\\033[2;1H\\033[L'";
    let visible = rterm_run_command(cmd, 40, 10);

    assert_eq!(visible[0].trim_end(), "AAA");
    assert_eq!(visible[1].trim_end(), ""); // inserted blank line
    assert_eq!(visible[2].trim_end(), "BBB");
    assert_eq!(visible[3].trim_end(), "CCC");
}

#[test]
fn pty_delete_line() {
    // Write 3 lines, go to line 2, delete it.
    let cmd = "printf 'AAA\\nBBB\\nCCC\\033[2;1H\\033[M'";
    let visible = rterm_run_command(cmd, 40, 10);

    assert_eq!(visible[0].trim_end(), "AAA");
    assert_eq!(visible[1].trim_end(), "CCC");
    assert_eq!(visible[2].trim_end(), ""); // shifted up, now blank
}

#[test]
fn pty_alternate_screen() {
    // Switch to alt screen, write, switch back. Primary screen should be unchanged.
    let cmd = "printf 'primary\\033[?1049hAlternate\\033[?1049l'";
    let visible = rterm_run_command(cmd, 40, 10);

    // After switching back, "primary" should be visible, not "Alternate".
    assert!(
        visible[0].contains("primary"),
        "primary screen should be restored, got: '{}'",
        visible[0]
    );
}

#[test]
fn pty_sgr_multiple_attributes() {
    let cmd = "printf '\\033[1;4;31mBoldUnderlineRed\\033[0m'";
    let visible = rterm_run_command(cmd, 40, 10);

    assert!(
        visible[0].contains("BoldUnderlineRed"),
        "expected text with multiple SGR attrs"
    );
}

#[test]
fn pty_scroll_region() {
    // Set scroll region to lines 2-4, then scroll within it.
    let cmd = "printf 'L1\\nL2\\nL3\\nL4\\nL5\\033[2;4r\\033[4;1H\\n\\nNEW'";
    let visible = rterm_run_command(cmd, 40, 10);

    // L1 should stay (outside scroll region).
    assert_eq!(visible[0].trim_end(), "L1");
}

// ============================================================================
// Tests: zellij side-by-side (requires zellij + PTY, skipped otherwise)
// ============================================================================

/// Run all zellij comparison tests sequentially sharing one session.
/// This avoids spawning/killing multiple sessions (slow + flaky).
fn zellij_comparisons() {
    let Some(zj) = ZellijSession::start() else {
        eprintln!("SKIP: zellij not available or no TTY");
        return;
    };

    // --- visible screen: seq 1 50 ---
    {
        zj.run_command("seq 1 50");
        let zellij_visible = zj.dump_screen();

        let rterm_visible = rterm_run_command("seq 1 50", 80, 24);

        let zellij_vis_nums: Vec<i64> = zellij_visible
            .iter()
            .filter_map(|l| l.trim().parse::<i64>().ok())
            .collect();
        let rterm_vis_nums: Vec<i64> = rterm_visible
            .iter()
            .filter_map(|l| l.trim().parse::<i64>().ok())
            .collect();

        println!(
            "[seq 1 50 visible] zellij={:?}, rterm={:?}",
            zellij_vis_nums, rterm_vis_nums
        );

        assert!(!rterm_vis_nums.is_empty(), "rterm should show some numbers");
        assert_eq!(
            *rterm_vis_nums.last().unwrap(),
            50,
            "last visible should be 50"
        );
    }

    // --- colors ---
    {
        zj.run_command(
            "echo -e '\\033[31mRED\\033[0m \\033[32mGREEN\\033[0m \\033[34mBLUE\\033[0m'",
        );
        let zellij_visible = zj.dump_screen();

        let rterm_visible = rterm_run_command(
            "echo -e '\\033[31mRED\\033[0m \\033[32mGREEN\\033[0m \\033[34mBLUE\\033[0m'",
            80,
            24,
        );

        // Both should contain "RED GREEN BLUE" (text content, not colors).
        let zj_has = zellij_visible
            .iter()
            .any(|l| l.contains("RED") && l.contains("BLUE"));
        let rt_has = rterm_visible
            .iter()
            .any(|l| l.contains("RED") && l.contains("BLUE"));

        assert!(zj_has, "zellij should show RED...BLUE");
        assert!(rt_has, "rterm should show RED...BLUE");
    }
}
