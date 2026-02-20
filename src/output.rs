use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;

use anyhow::Result;

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
    let mut f = std::fs::File::create(path)?;

    // Collect all range-spec entries
    let mut entries = Vec::new();
    for (line_idx, line) in screen.lines.iter().enumerate() {
        let line_num = line_idx + 1; // 1-based
        for span in &line.spans {
            // Escape | and \ in face strings
            let escaped_face = escape_face(&span.face);
            // Range format: "line.start_col,line.end_col|face"
            // end_byte is exclusive, but Kakoune range-specs uses inclusive end
            let end_byte_inclusive = span.end_byte - 1;
            entries.push(format!(
                "'{line_num}.{start},{line_num}.{end}|{face}'",
                start = span.start_byte,
                end = end_byte_inclusive,
                face = escaped_face,
            ));
        }
    }

    if entries.is_empty() {
        return Ok(());
    }

    // Write as set-option command(s), splitting if too large
    const MAX_CHUNK_SIZE: usize = 900_000; // ~900KB per command

    let mut chunk = String::from("set-option buffer scrollback_colors %val{timestamp}");
    for entry in &entries {
        if chunk.len() + 1 + entry.len() > MAX_CHUNK_SIZE && chunk.contains('\'') {
            // Write current chunk and start a new one with -add
            writeln!(f, "{chunk}")?;
            chunk = String::from("set-option -add buffer scrollback_colors");
        }
        write!(chunk, " {entry}")?;
    }
    if !chunk.is_empty() {
        writeln!(f, "{chunk}")?;
    }

    Ok(())
}

/// Escape | and \ in face strings for range-specs
fn escape_face(face: &str) -> String {
    let mut result = String::with_capacity(face.len());
    for ch in face.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '|' => result.push_str("\\|"),
            '\'' => result.push_str("''"),
            _ => result.push(ch),
        }
    }
    result
}

/// Generate Kakoune initialization script
pub fn write_init_kak(
    path: &Path,
    screen: &ProcessedScreen,
    window_id: &str,
    tmp_dir: &Path,
    ranges_path: &Path,
) -> Result<()> {
    let mut script = String::new();
    let tmp_dir_str = tmp_dir.display();
    let ranges_path_str = ranges_path.display();

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
        "set-option buffer scrollback_tmp_dir '{tmp_dir_str}'"
    )?;
    writeln!(script)?;

    // Range-specs declaration + apply
    writeln!(
        script,
        "declare-option -hidden range-specs scrollback_colors"
    )?;
    writeln!(
        script,
        "add-highlighter buffer/ ranges scrollback_colors"
    )?;
    writeln!(script, "source '{ranges_path_str}'")?;
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
        "        if [ -d '{tmp_dir_str}' ] && [ \"$(printf '%s' \"$kak_client_list\" | wc -w)\" -le 1 ]; then"
    )?;
    writeln!(
        script,
        "            echo \"nop %sh{{ rm -rf '{tmp_dir_str}' }}\""
    )?;
    writeln!(script, "        fi")?;
    writeln!(script, "    }}")?;
    writeln!(script, "}}")?;

    std::fs::write(path, &script)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{CursorPosition, ProcessedLine, ProcessedScreen, Span};

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
        write_init_kak(&path, &screen, "42", dir.path(), &ranges_path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("scrollback_kitty_window_id '42'"));
        assert!(content.contains("readonly true"));
        assert!(content.contains("select 5.3,5.3"));
        assert!(content.contains("execute-keys vb"));
        assert!(content.contains("kakoune-scrollback-setup-keymaps"));
        assert!(content.contains("ClientClose"));
        assert!(content.contains("rm -rf"));
    }
}
