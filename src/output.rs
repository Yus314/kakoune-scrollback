use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;

use anyhow::Result;

use crate::kitty::WindowId;
use crate::terminal::ProcessedScreen;

/// Generate plain text file
pub fn write_text(path: &Path, screen: &ProcessedScreen) -> Result<()> {
    let mut f = std::fs::File::create(path)?;
    for (i, line) in screen.lines.iter().enumerate() {
        if i > 0 {
            f.write_all(b"\n")?;
        }
        f.write_all(line.text.as_bytes())?;
    }
    // Ensure file ends with a newline
    if !screen.lines.is_empty() {
        f.write_all(b"\n")?;
    }
    Ok(())
}

/// Generate range-specs command file
pub fn write_ranges(path: &Path, screen: &ProcessedScreen) -> Result<()> {
    const MAX_CHUNK_SIZE: usize = 900_000; // ~900KB per command

    let mut f = std::fs::File::create(path)?;
    let mut chunk = String::with_capacity(MAX_CHUNK_SIZE);
    chunk.push_str("set-option buffer scrollback_colors %val{timestamp}");
    let mut chunk_has_entries = false;

    for (line_idx, line) in screen.lines.iter().enumerate() {
        let line_num = line_idx + 1; // 1-based
        for span in &line.spans {
            // Escape | and \ in face strings
            let escaped_face = escape_face(&span.face);
            // Range format: "line.start_col,line.end_col|face"
            // end_byte is exclusive, but Kakoune range-specs uses inclusive end
            let end_byte_inclusive = span.end_byte - 1;
            let entry = format!(
                "'{line_num}.{start},{line_num}.{end}|{face}'",
                start = span.start_byte,
                end = end_byte_inclusive,
                face = escaped_face,
            );

            // Flush chunk if adding this entry would exceed limit
            if chunk.len() + 1 + entry.len() > MAX_CHUNK_SIZE && chunk_has_entries {
                writeln!(f, "{chunk}")?;
                chunk.clear();
                chunk.push_str("set-option -add buffer scrollback_colors");
            }

            chunk.push(' ');
            chunk.push_str(&entry);
            chunk_has_entries = true;
        }
    }

    if chunk_has_entries {
        writeln!(f, "{chunk}")?;
    }

    Ok(())
}

/// Kakoune のシングルクォート文字列用エスケープ (' → '')
pub(crate) fn escape_kak_single_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// POSIX shell のシングルクォート文字列用エスケープ (' → '\'')
fn escape_shell_single_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Escape | and \ in face strings for range-specs
fn escape_face(face: &str) -> String {
    face.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\'', "''")
}

/// Generate Kakoune initialization script
pub fn write_init_kak(
    path: &Path,
    screen: &ProcessedScreen,
    window_id: WindowId,
    tmp_dir: &Path,
    ranges_path: &Path,
) -> Result<()> {
    let mut script = String::new();
    let tmp_dir_s = tmp_dir.display().to_string();
    let tmp_dir_kak = escape_kak_single_quote(&tmp_dir_s);
    let tmp_dir_sh = escape_shell_single_quote(&tmp_dir_s);
    let ranges_path_kak = escape_kak_single_quote(&ranges_path.display().to_string());

    // Global options (accessible from compose client too)
    writeln!(
        script,
        "set-option global scrollback_kitty_window_id '{window_id}'"
    )?;
    writeln!(script)?;

    // Buffer settings
    writeln!(script, "set-option buffer readonly true")?;
    writeln!(
        script,
        "set-option buffer scrollback_tmp_dir '{tmp_dir_kak}'"
    )?;
    writeln!(script)?;

    // Range-specs declaration + apply
    writeln!(
        script,
        "declare-option -hidden range-specs scrollback_colors"
    )?;
    writeln!(script, "add-highlighter buffer/ ranges scrollback_colors")?;
    writeln!(script, "source '{ranges_path_kak}'")?;
    writeln!(script, "update-option buffer scrollback_colors")?;
    writeln!(script)?;

    // Cursor position restore (calculated in Rust)
    writeln!(
        script,
        "select {line}.{col},{line}.{col}",
        line = screen.cursor.line,
        col = screen.cursor.col,
    )?;
    writeln!(script, "execute-keys vb")?;
    writeln!(script)?;

    // Enable keymaps
    writeln!(script, "kakoune-scrollback-setup-keymaps")?;
    writeln!(script)?;

    // Cleanup hook (guard: don't fire when compose client closes)
    writeln!(script, "hook -always global ClientClose .* %{{")?;
    writeln!(script, "    evaluate-commands %sh{{")?;
    writeln!(
        script,
        "        if [ -d '{tmp_dir_sh}' ] && [ \"$(printf '%s' \"$kak_client_list\" | wc -w)\" -le 1 ]; then"
    )?;
    writeln!(script, "            rm -rf -- '{tmp_dir_sh}'")?;
    writeln!(script, "        fi")?;
    writeln!(script, "    }}")?;
    writeln!(script, "}}")?;

    std::fs::write(path, &script)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kitty;
    use crate::terminal::{CursorPosition, ProcessedLine, ProcessedScreen, Span};

    fn wid(s: &str) -> kitty::WindowId {
        kitty::parse_window_id(s).unwrap()
    }

    fn make_screen(lines: Vec<ProcessedLine>, cursor: CursorPosition) -> ProcessedScreen {
        ProcessedScreen { lines, cursor }
    }

    #[test]
    fn write_text_basic() {
        let screen = make_screen(
            vec![
                ProcessedLine {
                    text: "Hello".to_string(),
                    spans: vec![],
                },
                ProcessedLine {
                    text: "World".to_string(),
                    spans: vec![],
                },
            ],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.txt");
        write_text(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello\nWorld\n");
    }

    #[test]
    fn write_ranges_basic() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "Hello World".to_string(),
                spans: vec![Span {
                    start_byte: 1,
                    end_byte: 6,
                    face: "rgb:FF0000,default+b".to_string(),
                }],
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("set-option buffer scrollback_colors"));
        assert!(content.contains("1.1,1.5|rgb:FF0000,default+b"));
    }

    #[test]
    fn write_ranges_empty() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "plain".to_string(),
                spans: vec![],
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn escape_face_special_chars() {
        assert_eq!(escape_face("rgb:FF|00"), "rgb:FF\\|00");
        assert_eq!(escape_face("a\\b"), "a\\\\b");
        assert_eq!(escape_face("normal"), "normal");
    }

    #[test]
    fn write_init_kak_contains_required_elements() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "test".to_string(),
                spans: vec![],
            }],
            CursorPosition { line: 5, col: 3 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("init.kak");
        let ranges_path = dir.path().join("ranges.kak");
        write_init_kak(&path, &screen, wid("42"), dir.path(), &ranges_path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("scrollback_kitty_window_id '42'"));
        assert!(content.contains("readonly true"));
        assert!(content.contains("select 5.3,5.3"));
        assert!(content.contains("execute-keys vb"));
        assert!(content.contains("kakoune-scrollback-setup-keymaps"));
        assert!(content.contains("ClientClose"));
        assert!(content.contains("rm -rf --"));
    }

    // --- Phase 1: HIGH priority ---

    #[test]
    fn write_ranges_chunking() {
        const MAX_CHUNK_SIZE: usize = 900_000;
        let face = "rgb:FF0000,default".to_string();

        // All entries on line 1 → uniform entry '1.1,1.1|rgb:FF0000,default'
        let sample = format!("'1.1,1.1|{face}'");
        let num_spans = MAX_CHUNK_SIZE / (sample.len() + 1) + 2;

        let screen = make_screen(
            vec![ProcessedLine {
                text: "x".to_string(),
                spans: (0..num_spans)
                    .map(|_| Span {
                        start_byte: 1,
                        end_byte: 2,
                        face: face.clone(),
                    })
                    .collect(),
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        let output_lines: Vec<&str> = content.lines().collect();
        assert!(
            output_lines.len() >= 2,
            "Expected chunked output, got {} lines",
            output_lines.len()
        );
        // First chunk has %val{timestamp}
        assert!(output_lines[0].contains("%val{timestamp}"));
        // Second chunk uses -add
        assert!(output_lines[1].contains("-add"));
    }

    #[test]
    fn span_end_byte_exclusive_to_inclusive() {
        use crate::kitty::KittyPipeData;
        use crate::terminal;

        let input = b"\x1b[31mHello\x1b[0m World";
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let screen = terminal::process_bytes(
            &pd,
            input,
            &crate::palette::DEFAULT_PALETTE,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );

        // terminal.rs produces exclusive end_byte
        let span = &screen.lines[0].spans[0];
        assert_eq!(span.start_byte, 1);
        assert_eq!(span.end_byte, 6); // exclusive: "Hello" = 5 bytes

        // write_ranges converts to inclusive end (end_byte - 1)
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        // Output should use inclusive end: 6 - 1 = 5
        assert!(content.contains("1.1,1.5|"));
    }

    // --- Phase 2: MEDIUM priority ---

    #[test]
    fn write_text_empty_screen() {
        let screen = make_screen(vec![], CursorPosition { line: 1, col: 1 });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.txt");
        write_text(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn write_ranges_multiple_spans_same_line() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "RedGreenBlue".to_string(),
                spans: vec![
                    Span {
                        start_byte: 1,
                        end_byte: 4,
                        face: "rgb:FF0000,default".to_string(),
                    },
                    Span {
                        start_byte: 4,
                        end_byte: 9,
                        face: "rgb:00FF00,default".to_string(),
                    },
                    Span {
                        start_byte: 9,
                        end_byte: 13,
                        face: "rgb:0000FF,default".to_string(),
                    },
                ],
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        // All spans should be in one set-option command
        assert_eq!(content.lines().count(), 1);
        assert!(content.contains("1.1,1.3|rgb:FF0000,default"));
        assert!(content.contains("1.4,1.8|rgb:00FF00,default"));
        assert!(content.contains("1.9,1.12|rgb:0000FF,default"));
    }

    #[test]
    fn write_ranges_multiple_lines() {
        let screen = make_screen(
            vec![
                ProcessedLine {
                    text: "Red".to_string(),
                    spans: vec![Span {
                        start_byte: 1,
                        end_byte: 4,
                        face: "rgb:FF0000,default".to_string(),
                    }],
                },
                ProcessedLine {
                    text: "Green".to_string(),
                    spans: vec![Span {
                        start_byte: 1,
                        end_byte: 6,
                        face: "rgb:00FF00,default".to_string(),
                    }],
                },
                ProcessedLine {
                    text: "Blue".to_string(),
                    spans: vec![Span {
                        start_byte: 1,
                        end_byte: 5,
                        face: "rgb:0000FF,default".to_string(),
                    }],
                },
            ],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.kak");
        write_ranges(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("1.1,1.3|rgb:FF0000,default"));
        assert!(content.contains("2.1,2.5|rgb:00FF00,default"));
        assert!(content.contains("3.1,3.4|rgb:0000FF,default"));
    }

    // --- Phase 3: LOW priority ---

    #[test]
    fn write_text_single_empty_line() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "".to_string(),
                spans: vec![],
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.txt");
        write_text(&path, &screen).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "\n");
    }

    #[test]
    fn escape_face_single_quote() {
        assert_eq!(escape_face("it's"), "it''s");
    }

    // --- Path injection prevention ---

    #[test]
    fn escape_kak_single_quote_normal() {
        assert_eq!(escape_kak_single_quote("/tmp/foo"), "/tmp/foo");
    }

    #[test]
    fn escape_kak_single_quote_with_quote() {
        assert_eq!(escape_kak_single_quote("/tmp/it's"), "/tmp/it''s");
    }

    #[test]
    fn escape_kak_single_quote_empty() {
        assert_eq!(escape_kak_single_quote(""), "");
    }

    #[test]
    fn escape_shell_single_quote_normal() {
        assert_eq!(escape_shell_single_quote("/tmp/foo"), "/tmp/foo");
    }

    #[test]
    fn escape_shell_single_quote_with_quote() {
        assert_eq!(escape_shell_single_quote("/tmp/it's"), "/tmp/it'\\''s");
    }

    #[test]
    fn escape_shell_single_quote_with_command_substitution() {
        assert_eq!(
            escape_shell_single_quote("/tmp/$(rm -rf /)"),
            "/tmp/$(rm -rf /)"
        );
    }

    #[test]
    fn write_init_kak_escapes_single_quotes_in_paths() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "test".to_string(),
                spans: vec![],
            }],
            CursorPosition { line: 1, col: 1 },
        );
        let dir = tempfile::tempdir().unwrap();
        // Create a subdirectory with a single quote in the name
        let evil_dir = dir.path().join("it's-a-dir");
        std::fs::create_dir_all(&evil_dir).unwrap();
        let path = evil_dir.join("init.kak");
        let ranges_path = evil_dir.join("ranges.kak");
        write_init_kak(&path, &screen, wid("42"), &evil_dir, &ranges_path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        // Kakoune contexts should use '' escaping
        assert!(
            content.contains("scrollback_tmp_dir 'it''s-a-dir'") || content.contains("it''s-a-dir"),
            "Kakoune single quotes should be escaped with '', got:\n{content}"
        );

        // Shell contexts should use '\'' escaping
        assert!(
            content.contains("it'\\''s-a-dir"),
            "Shell single quotes should be escaped with '\\''', got:\n{content}"
        );

        // The dangerous 3-layer nesting pattern should not exist
        assert!(
            !content.contains("echo \"nop %sh{"),
            "Should not contain nested echo nop %sh pattern, got:\n{content}"
        );

        // rm -rf should use -- for safety
        assert!(
            content.contains("rm -rf --"),
            "rm -rf should use -- flag, got:\n{content}"
        );
    }

    #[test]
    fn write_init_kak_large_cursor_coords() {
        let screen = make_screen(
            vec![ProcessedLine {
                text: "test".to_string(),
                spans: vec![],
            }],
            CursorPosition {
                line: 1000,
                col: 50,
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("init.kak");
        let ranges_path = dir.path().join("ranges.kak");
        write_init_kak(&path, &screen, wid("42"), dir.path(), &ranges_path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("select 1000.50,1000.50"));
    }
}
