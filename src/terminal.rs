use crate::kitty::KittyPipeData;
use crate::palette;

/// Cursor position (calculated as Kakoune byte offset)
pub struct CursorPosition {
    pub line: usize, // 1-based line number in buffer
    pub col: usize,  // 1-based byte offset
}

pub struct ProcessedScreen {
    pub lines: Vec<ProcessedLine>,
    pub cursor: CursorPosition,
}

pub struct ProcessedLine {
    pub text: String,
    pub spans: Vec<Span>,
}

pub struct Span {
    pub start_byte: usize, // 1-based byte offset
    pub end_byte: usize,   // 1-based byte offset (exclusive)
    pub face: String,      // Kakoune face string (e.g. "rgb:FF0000,default+bi")
}

pub(crate) const DEFAULT_MAX_SCROLLBACK_LINES: usize = 200_000;

/// Process from byte slice directly
pub fn process_bytes(
    pipe_data: &KittyPipeData,
    data: &[u8],
    palette: &[u8; 48],
    max_scrollback_lines: usize,
) -> ProcessedScreen {
    let rows = pipe_data.lines;
    let cols = pipe_data.columns;

    let mut parser = vt100::Parser::new(rows, cols, max_scrollback_lines);
    parser.process(data);

    let screen = parser.screen_mut();

    // Find total scrollback lines
    screen.set_scrollback(usize::MAX);
    let total_sb = screen.scrollback();

    let mut lines = Vec::new();
    let mut cursor = CursorPosition { line: 1, col: 1 };

    // The cursor in the output buffer is at line (total_sb + cursor_y + 1), 1-based
    let cursor_output_line = total_sb + pipe_data.cursor_y + 1;

    // Read initial screen rows from the max scrollback offset
    screen.set_scrollback(total_sb);
    for row in 0..rows {
        push_row(
            screen,
            row,
            pipe_data,
            cursor_output_line,
            &mut lines,
            &mut cursor,
            palette,
        );
    }

    // Read one new line at the bottom for each offset decrease
    for offset in (0..total_sb).rev() {
        screen.set_scrollback(offset);
        push_row(
            screen,
            rows - 1,
            pipe_data,
            cursor_output_line,
            &mut lines,
            &mut cursor,
            palette,
        );
    }

    // Trim trailing empty lines
    while lines
        .last()
        .is_some_and(|l| l.text.is_empty() && l.spans.is_empty())
    {
        lines.pop();
    }

    // Clamp cursor if lines were trimmed
    if cursor.line > lines.len() {
        cursor.line = lines.len().max(1);
        cursor.col = 1;
    }

    ProcessedScreen { lines, cursor }
}

fn push_row(
    screen: &vt100::Screen,
    row: u16,
    pipe_data: &KittyPipeData,
    cursor_output_line: usize,
    lines: &mut Vec<ProcessedLine>,
    cursor: &mut CursorPosition,
    palette: &[u8; 48],
) {
    let line_idx = lines.len();
    let is_cursor_line = line_idx + 1 == cursor_output_line;
    let pline = process_row(
        screen,
        row,
        pipe_data.columns,
        pipe_data,
        is_cursor_line,
        cursor,
        palette,
    );
    lines.push(pline);
    if is_cursor_line {
        cursor.line = line_idx + 1;
    }
}

fn process_row(
    screen: &vt100::Screen,
    row: u16,
    cols: u16,
    pipe_data: &KittyPipeData,
    is_cursor_line: bool,
    cursor: &mut CursorPosition,
    palette: &[u8; 48],
) -> ProcessedLine {
    let mut text = String::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut current_face: Option<String> = None;
    let mut span_start_byte: usize = 1; // 1-based

    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            break;
        };

        // Skip wide continuation cells
        if cell.is_wide_continuation() {
            continue;
        }

        let contents = cell.contents();
        let byte_offset_before = text.len(); // 0-based

        // Track cursor column (byte offset)
        if is_cursor_line && usize::from(col) == pipe_data.cursor_x {
            cursor.col = byte_offset_before + 1; // 1-based
        }

        // Append cell content (or space if empty)
        if contents.is_empty() {
            text.push(' ');
        } else {
            text.push_str(contents);
        }

        // Compute face for this cell
        let face = cell_face(cell, palette);

        if face != current_face {
            // Close previous span if any
            let byte_now = byte_offset_before + 1; // 1-based
            if let Some(f) = current_face.take() {
                if span_start_byte < byte_now {
                    spans.push(Span {
                        start_byte: span_start_byte,
                        end_byte: byte_now,
                        face: f,
                    });
                }
            }
            current_face = face;
            span_start_byte = byte_now;
        }
    }

    // Close final span
    let byte_end = text.len() + 1; // 1-based, exclusive
    if let Some(f) = current_face {
        if span_start_byte < byte_end {
            spans.push(Span {
                start_byte: span_start_byte,
                end_byte: byte_end,
                face: f,
            });
        }
    }

    // Trim trailing spaces from text
    let trimmed_len = text.trim_end().len();
    if trimmed_len < text.len() {
        text.truncate(trimmed_len);
        // Adjust spans that extend beyond trimmed text
        let max_byte = trimmed_len + 1; // 1-based exclusive
        spans.retain(|s| s.start_byte < max_byte);
        if let Some(last) = spans.last_mut() {
            if last.end_byte > max_byte {
                last.end_byte = max_byte;
            }
        }
    }

    ProcessedLine { text, spans }
}

fn cell_face(cell: &vt100::Cell, palette: &[u8; 48]) -> Option<String> {
    let fg = palette::color_to_kak(cell.fgcolor(), palette);
    let bg = palette::color_to_kak(cell.bgcolor(), palette);

    let mut attrs = String::new();
    if cell.bold() {
        attrs.push('b');
    }
    if cell.dim() {
        attrs.push('d');
    }
    if cell.italic() {
        attrs.push('i');
    }
    if cell.underline() {
        attrs.push('u');
    }
    if cell.inverse() {
        attrs.push('r');
    }

    if fg.is_none() && bg.is_none() && attrs.is_empty() {
        return None;
    }

    let fg_str = fg.as_deref().unwrap_or("default");
    let bg_str = bg.as_deref().unwrap_or("default");
    let attr_str = if attrs.is_empty() {
        String::new()
    } else {
        format!("+{attrs}")
    };
    Some(format!("{fg_str},{bg_str}{attr_str}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kitty::KittyPipeData;

    fn default_pipe_data() -> KittyPipeData {
        KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        }
    }

    #[test]
    fn process_plain_text() {
        let input = b"Hello World";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Hello World");
        assert_eq!(screen.lines[0].spans.len(), 0);
    }

    #[test]
    fn process_colored_text() {
        let input = b"\x1b[31mHello\x1b[0m World";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Hello World");
        assert!(screen.lines[0].spans.len() >= 1);
        assert!(screen.lines[0].spans[0].face.contains("rgb:"));
    }

    #[test]
    fn default_spans_skipped() {
        let input = b"plain text";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].spans.len(), 0);
    }

    #[test]
    fn span_merging() {
        // Entire "Hello World" in red — should be 1 span
        let input = b"\x1b[31mHello World\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Hello World");
        assert_eq!(screen.lines[0].spans.len(), 1);
    }

    #[test]
    fn wide_characters() {
        let input = "日本語test".as_bytes();
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "日本語test");
    }

    #[test]
    fn wide_char_byte_offsets() {
        // "日" = 3 bytes, "本" = 3 bytes, "語" = 3 bytes
        let input = b"\x1b[31m\xe6\x97\xa5\x1b[0m\xe6\x9c\xac"; // red "日" + normal "本"
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "日本");
        assert_eq!(screen.lines[0].spans.len(), 1);
        // Red span covers "日" = bytes 1..4 (1-based, exclusive end)
        assert_eq!(screen.lines[0].spans[0].start_byte, 1);
        assert_eq!(screen.lines[0].spans[0].end_byte, 4); // 3 bytes + 1
    }

    #[test]
    fn attributes_combined() {
        let input = b"\x1b[1;3mBoldItalic\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert!(screen.lines[0].spans[0].face.contains("+bi"));
    }

    #[test]
    fn cursor_position_simple() {
        let input = b"line1\r\nline2\r\nline3";
        let pd = KittyPipeData {
            cursor_x: 3,
            cursor_y: 2,
            lines: 24,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // Cursor should be on line 3 (1-based), col 4 (1-based, after "lin")
        assert_eq!(screen.cursor.line, 3);
        assert_eq!(screen.cursor.col, 4);
    }

    #[test]
    fn empty_input() {
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            b"",
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // Should have no lines (all trimmed as empty)
        assert!(screen.lines.is_empty() || screen.lines.iter().all(|l| l.text.is_empty()));
    }

    #[test]
    fn scrollback_lines() {
        // Generate enough output to create scrollback
        let mut input = Vec::new();
        for i in 0..30 {
            input.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 10,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            &input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // Should have all 30 lines (some in scrollback, some on screen)
        assert!(screen.lines.len() >= 30);
        assert_eq!(screen.lines[0].text, "line 0");
        assert_eq!(screen.lines[29].text, "line 29");
    }

    // --- Phase 1: HIGH priority ---

    #[test]
    fn cursor_position_with_scrollback() {
        let mut input = Vec::new();
        for i in 0..30 {
            input.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 5,
            lines: 10,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            &input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // 30 lines + trailing \r\n = 31 rows, total_sb = 21
        // cursor_output_line = 21 + 5 + 1 = 27
        assert_eq!(screen.cursor.line, 27);
    }

    #[test]
    fn cursor_clamped_on_trimmed_empty_lines() {
        let input = b"Hello";
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 23,
            lines: 24,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // Only line 0 has text "Hello", lines 1-23 are empty and get trimmed
        // Cursor was at line 24 (1-based), gets clamped to last line
        assert_eq!(screen.lines.len(), 1);
        assert_eq!(screen.cursor.line, 1);
        assert_eq!(screen.cursor.col, 1);
    }

    #[test]
    fn multiple_colors_same_line() {
        let input = b"\x1b[31mRed\x1b[32mGreen\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "RedGreen");
        assert_eq!(screen.lines[0].spans.len(), 2);
        // Red span: bytes 1..4 (exclusive), covers "Red"
        assert_eq!(screen.lines[0].spans[0].start_byte, 1);
        assert_eq!(screen.lines[0].spans[0].end_byte, 4);
        // Green span: bytes 4..9 (exclusive), covers "Green"
        assert_eq!(screen.lines[0].spans[1].start_byte, 4);
        assert_eq!(screen.lines[0].spans[1].end_byte, 9);
    }

    #[test]
    fn background_color_only() {
        let input = b"\x1b[42mHighlighted\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Highlighted");
        assert_eq!(screen.lines[0].spans.len(), 1);
        // SGR 42 = green background, default foreground
        assert_eq!(screen.lines[0].spans[0].face, "default,rgb:00CC00");
    }

    #[test]
    fn span_adjustment_on_trailing_trim() {
        // Red "Hi" followed by red trailing spaces that get trimmed
        let input = b"\x1b[31mHi   \x1b[0m";
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 20,
        };
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Hi");
        assert_eq!(screen.lines[0].spans.len(), 1);
        // Span should be trimmed to text length: bytes 1..3 (exclusive)
        assert_eq!(screen.lines[0].spans[0].start_byte, 1);
        assert_eq!(screen.lines[0].spans[0].end_byte, 3);
    }

    // --- Phase 2: MEDIUM priority ---

    #[test]
    fn attribute_dim() {
        let input = b"\x1b[2mDim\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Dim");
        assert!(screen.lines[0].spans[0].face.contains("+d"));
    }

    #[test]
    fn attribute_underline() {
        let input = b"\x1b[4mUnderlined\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Underlined");
        assert!(screen.lines[0].spans[0].face.contains("+u"));
    }

    #[test]
    fn attribute_inverse() {
        let input = b"\x1b[7mReversed\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Reversed");
        assert!(screen.lines[0].spans[0].face.contains("+r"));
    }

    #[test]
    fn reset_then_new_color() {
        let input = b"\x1b[31mR\x1b[0m N \x1b[34mB\x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "R N B");
        assert_eq!(screen.lines[0].spans.len(), 2);
        // Red span for "R"
        assert_eq!(screen.lines[0].spans[0].start_byte, 1);
        assert_eq!(screen.lines[0].spans[0].end_byte, 2);
        // Blue span for "B"
        assert_eq!(screen.lines[0].spans[1].start_byte, 5);
        assert_eq!(screen.lines[0].spans[1].end_byte, 6);
    }

    #[test]
    fn line_with_only_formatting_no_visible_text() {
        let input = b"\x1b[31m   \x1b[0m";
        let pd = default_pipe_data();
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // All text was colored spaces, trimmed to empty; all empty lines removed
        assert!(screen.lines.is_empty());
    }

    #[test]
    fn cursor_on_wide_character() {
        let input = "日test".as_bytes();
        let pd = KittyPipeData {
            cursor_x: 2,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "日test");
        // "日" = 3 bytes (occupies cols 0-1), cursor_x:2 = 't'
        // byte offset = 3 (0-based) + 1 = 4 (1-based)
        assert_eq!(screen.cursor.col, 4);
    }

    #[test]
    fn scrollback_cursor_position_variants() {
        let mut input = Vec::new();
        for i in 0..30 {
            input.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }

        // Cursor at first visible line (cursor_y:0)
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 10,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            &input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // 30 lines + trailing \r\n = 31 rows, total_sb = 21
        // cursor_output_line = 21 + 0 + 1 = 22
        assert_eq!(screen.cursor.line, 22);

        // Cursor at last visible line (cursor_y:9)
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 9,
            lines: 10,
            columns: 80,
        };
        let screen = process_bytes(
            &pd,
            &input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        // cursor_output_line = 21 + 9 + 1 = 31, but trailing empty line trimmed
        // lines.len() = 30, so cursor clamped to 30 with col = 1
        assert_eq!(screen.cursor.line, 30);
        assert_eq!(screen.cursor.col, 1);
    }

    #[test]
    fn trailing_spaces_trimmed() {
        let input = b"Hello";
        let pd = default_pipe_data(); // 80 columns
        let screen = process_bytes(
            &pd,
            input,
            &palette::DEFAULT_PALETTE,
            DEFAULT_MAX_SCROLLBACK_LINES,
        );
        assert_eq!(screen.lines[0].text, "Hello");
        assert_eq!(screen.lines[0].text.len(), 5);
    }

    #[test]
    fn small_max_scrollback_truncates_old_lines() {
        let mut input = Vec::new();
        for i in 0..30 {
            input.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 10,
            columns: 80,
        };
        // max_scrollback_lines=5 → scrollback buffer limited to 5 lines
        let screen = process_bytes(&pd, &input, &palette::DEFAULT_PALETTE, 5);
        // Oldest lines should be truncated
        assert_ne!(screen.lines[0].text, "line 0");
    }

    #[test]
    fn cursor_clamped_when_scrollback_truncated() {
        let mut input = Vec::new();
        for i in 0..30 {
            input.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 5,
            lines: 10,
            columns: 80,
        };
        let screen = process_bytes(&pd, &input, &palette::DEFAULT_PALETTE, 5);
        // Cursor line should not exceed lines.len()
        assert!(screen.cursor.line <= screen.lines.len());
        assert!(screen.cursor.line >= 1);
    }
}
