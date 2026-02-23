use std::fmt;

use anyhow::{bail, Context, Result};

use crate::palette;

/// A validated Kitty window ID (non-zero u32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(u32);

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct PipeData {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub lines: u16,
    pub columns: u16,
}

/// Pure function: parse from string (separated for testability)
/// Format (Kitty): `{scrolled_by}:{cursor_x},{cursor_y}:{lines},{columns}`
/// cursor_x, cursor_y are 1-based (top-left = 1,1); converted to 0-based internally.
pub fn parse_pipe_data_str(s: &str) -> Result<PipeData> {
    let s = s.trim();

    let (part0, rest) = s
        .split_once(':')
        .context("KITTY_PIPE_DATA: expected 3 colon-separated parts")?;
    let (part1, part2) = rest
        .split_once(':')
        .context("KITTY_PIPE_DATA: expected 3 colon-separated parts")?;
    if part2.contains(':') {
        bail!("KITTY_PIPE_DATA: expected 3 colon-separated parts");
    }

    // part0: scrolled_by (validate as numeric, don't use)
    if part0.contains(',') {
        bail!("KITTY_PIPE_DATA: invalid scrolled_by '{part0}' (unexpected comma)");
    }
    let _scrolled_by: usize = part0
        .parse()
        .context("KITTY_PIPE_DATA: invalid scrolled_by")?;

    // part1: cursor_x,cursor_y (1-based)
    let (cx_str, cy_str) = part1.split_once(',').with_context(|| {
        format!("KITTY_PIPE_DATA: expected 'cursor_x,cursor_y' in second part, got '{part1}'")
    })?;
    if cy_str.contains(',') {
        bail!("KITTY_PIPE_DATA: expected 'cursor_x,cursor_y' in second part, got '{part1}'");
    }
    let cursor_x_1: usize = cx_str
        .parse()
        .context("KITTY_PIPE_DATA: invalid cursor_x")?;
    let cursor_y_1: usize = cy_str
        .parse()
        .context("KITTY_PIPE_DATA: invalid cursor_y")?;

    // part2: lines,columns
    let (lines_str, columns_str) = part2.split_once(',').with_context(|| {
        format!("KITTY_PIPE_DATA: expected 'lines,columns' in third part, got '{part2}'")
    })?;
    if columns_str.contains(',') {
        bail!("KITTY_PIPE_DATA: expected 'lines,columns' in third part, got '{part2}'");
    }

    let lines: u16 = lines_str
        .parse()
        .context("KITTY_PIPE_DATA: invalid lines")?;
    let columns: u16 = columns_str
        .parse()
        .context("KITTY_PIPE_DATA: invalid columns")?;

    if lines == 0 {
        bail!("KITTY_PIPE_DATA: lines must be at least 1");
    }
    if columns == 0 {
        bail!("KITTY_PIPE_DATA: columns must be at least 1");
    }
    if cursor_x_1 == 0 {
        bail!("KITTY_PIPE_DATA: cursor_x must be at least 1 (1-based)");
    }
    if cursor_y_1 == 0 {
        bail!("KITTY_PIPE_DATA: cursor_y must be at least 1 (1-based)");
    }
    if cursor_x_1 > usize::from(columns) {
        bail!("KITTY_PIPE_DATA: cursor_x ({cursor_x_1}) must be at most columns ({columns})");
    }
    if cursor_y_1 > usize::from(lines) {
        bail!("KITTY_PIPE_DATA: cursor_y ({cursor_y_1}) must be at most lines ({lines})");
    }

    Ok(PipeData {
        cursor_x: cursor_x_1 - 1,
        cursor_y: cursor_y_1 - 1,
        lines,
        columns,
    })
}

/// Read `KITTY_PIPE_DATA` environment variable and delegate to `parse_pipe_data_str`
pub fn parse_pipe_data() -> Result<PipeData> {
    let val =
        std::env::var("KITTY_PIPE_DATA").context("KITTY_PIPE_DATA environment variable not set")?;
    parse_pipe_data_str(&val)
}

/// Pure function: validate and parse a kitty window ID string (separated for testability)
pub fn parse_window_id(s: &str) -> Result<WindowId> {
    let id: u32 = s.parse().with_context(|| {
        format!("invalid kitty window ID '{s}' — expected a number (check your kitty.conf)")
    })?;
    if id == 0 {
        bail!("invalid kitty window ID '0' — window IDs start at 1");
    }
    Ok(WindowId(id))
}

/// Query the running Kitty instance for its color palette.
/// Falls back to `DEFAULT_PALETTE` with a warning if the command fails.
pub fn get_palette(window_id: WindowId) -> [u8; 48] {
    let output = std::process::Command::new("kitty")
        .args(["@", "get-colors", "--match", &format!("id:{window_id}")])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            palette::parse_kitty_colors(&text)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!(
                "warning: kitty @ get-colors failed ({}): {}\nUsing default palette.",
                out.status,
                stderr.trim(),
            );
            palette::DEFAULT_PALETTE
        }
        Err(e) => {
            eprintln!("warning: failed to run kitty @ get-colors: {e}\nUsing default palette.");
            palette::DEFAULT_PALETTE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pipe_data_valid() {
        let data = parse_pipe_data_str("42:6,24:50,120").unwrap();
        assert_eq!(data.cursor_x, 5);
        assert_eq!(data.cursor_y, 23);
        assert_eq!(data.lines, 50);
        assert_eq!(data.columns, 120);
    }

    #[test]
    fn parse_pipe_data_zeros() {
        let data = parse_pipe_data_str("0:1,1:24,80").unwrap();
        assert_eq!(data.cursor_x, 0);
        assert_eq!(data.cursor_y, 0);
        assert_eq!(data.lines, 24);
        assert_eq!(data.columns, 80);
    }

    #[test]
    fn parse_pipe_data_no_scroll() {
        let data = parse_pipe_data_str("0:4,11:24,80").unwrap();
        assert_eq!(data.cursor_x, 3);
        assert_eq!(data.cursor_y, 10);
    }

    #[test]
    fn parse_pipe_data_invalid() {
        assert!(parse_pipe_data_str("invalid").is_err());
        assert!(parse_pipe_data_str("").is_err());
        assert!(parse_pipe_data_str("1,2:3").is_err()); // missing third part
    }

    #[test]
    fn parse_pipe_data_non_numeric() {
        assert!(parse_pipe_data_str("abc:1,1:24,80").is_err());
    }

    #[test]
    fn parse_window_id_valid() {
        assert_eq!(parse_window_id("42").unwrap(), WindowId(42));
        assert_eq!(parse_window_id("1").unwrap(), WindowId(1));
    }

    #[test]
    fn parse_window_id_normalizes() {
        assert_eq!(parse_window_id("042").unwrap(), WindowId(42));
    }

    #[test]
    fn window_id_display() {
        assert_eq!(WindowId(42).to_string(), "42");
        assert_eq!(WindowId(1).to_string(), "1");
    }

    #[test]
    fn parse_window_id_rejects_zero() {
        assert!(parse_window_id("0").is_err());
    }

    #[test]
    fn parse_window_id_rejects_unexpanded_env_var() {
        assert!(parse_window_id("$KITTY_WINDOW_ID").is_err());
    }

    #[test]
    fn parse_window_id_rejects_unexpanded_placeholder() {
        assert!(parse_window_id("@active-kitty-window-id").is_err());
    }

    #[test]
    fn parse_window_id_rejects_empty() {
        assert!(parse_window_id("").is_err());
    }

    #[test]
    fn parse_window_id_rejects_non_numeric() {
        assert!(parse_window_id("abc").is_err());
    }

    #[test]
    fn parse_pipe_data_rejects_zero_lines() {
        let err = parse_pipe_data_str("0:1,1:0,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(msg.contains("lines"), "error should mention lines: {msg}");
    }

    #[test]
    fn parse_pipe_data_rejects_zero_columns() {
        let err = parse_pipe_data_str("0:1,1:24,0");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("columns"),
            "error should mention columns: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_rejects_cursor_y_out_of_range() {
        let err = parse_pipe_data_str("0:1,25:24,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("cursor_y"),
            "error should mention cursor_y: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_rejects_cursor_x_out_of_range() {
        let err = parse_pipe_data_str("0:81,1:24,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("cursor_x"),
            "error should mention cursor_x: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_cursor_at_max_valid_position() {
        let data = parse_pipe_data_str("0:80,24:24,80").unwrap();
        assert_eq!(data.cursor_x, 79);
        assert_eq!(data.cursor_y, 23);
    }

    #[test]
    fn parse_pipe_data_rejects_huge_cursor_y() {
        let err = parse_pipe_data_str("0:1,9999:24,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("cursor_y"),
            "error should mention cursor_y: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_rejects_cursor_x_zero() {
        let err = parse_pipe_data_str("0:0,1:24,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("cursor_x"),
            "error should mention cursor_x: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_rejects_cursor_y_zero() {
        let err = parse_pipe_data_str("0:1,0:24,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("KITTY_PIPE_DATA:"),
            "error should have standard prefix: {msg}"
        );
        assert!(
            msg.contains("cursor_y"),
            "error should mention cursor_y: {msg}"
        );
    }

    #[test]
    fn parse_pipe_data_rejects_missing_cursor_comma() {
        assert!(parse_pipe_data_str("0:1:24,80").is_err());
    }

    #[test]
    fn parse_pipe_data_rejects_extra_cursor_comma() {
        assert!(parse_pipe_data_str("0:1,2,3:24,80").is_err());
    }

    #[test]
    fn parse_pipe_data_rejects_non_numeric_cursor_x() {
        assert!(parse_pipe_data_str("0:x,1:24,80").is_err());
    }

    #[test]
    fn parse_pipe_data_rejects_non_numeric_cursor_y() {
        assert!(parse_pipe_data_str("0:1,y:24,80").is_err());
    }

    #[test]
    fn parse_pipe_data_trims_whitespace() {
        let data = parse_pipe_data_str("0:1,1:24,80\n").unwrap();
        assert_eq!(data.cursor_x, 0);
        assert_eq!(data.cursor_y, 0);
        assert_eq!(data.lines, 24);
        assert_eq!(data.columns, 80);
    }
}
