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
pub struct KittyPipeData {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub lines: u16,
    pub columns: u16,
}

/// Pure function: parse from string (separated for testability)
/// Format: `scrolled_by[,cursor_x]:cursor_y:lines,columns`
pub fn parse_pipe_data_str(s: &str) -> Result<KittyPipeData> {
    let (part0, rest) = s
        .split_once(':')
        .context("KITTY_PIPE_DATA: expected 3 colon-separated parts")?;
    let (part1, part2) = rest
        .split_once(':')
        .context("KITTY_PIPE_DATA: expected 3 colon-separated parts")?;
    if part2.contains(':') {
        bail!("KITTY_PIPE_DATA: expected 3 colon-separated parts");
    }

    let (scrolled_by_str, cursor_x_str) = match part0.split_once(',') {
        None => (part0, "0"),
        Some((a, b)) => {
            if b.contains(',') {
                bail!("KITTY_PIPE_DATA: invalid first part '{part0}'");
            }
            (a, b)
        }
    };

    // Validate scrolled_by is numeric even though we don't use the value
    let _scrolled_by: usize = scrolled_by_str
        .parse()
        .context("KITTY_PIPE_DATA: invalid scrolled_by")?;

    let cursor_y: usize = part1
        .split(',')
        .next()
        .context("KITTY_PIPE_DATA: missing cursor_y")?
        .parse()
        .context("KITTY_PIPE_DATA: invalid cursor_y")?;

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

    Ok(KittyPipeData {
        cursor_x: cursor_x_str
            .parse()
            .context("KITTY_PIPE_DATA: invalid cursor_x")?,
        cursor_y,
        lines,
        columns,
    })
}

/// Read `KITTY_PIPE_DATA` environment variable and delegate to `parse_pipe_data_str`
pub fn parse_pipe_data() -> Result<KittyPipeData> {
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
        let data = parse_pipe_data_str("42,5:23:50,120").unwrap();
        assert_eq!(data.cursor_x, 5);
        assert_eq!(data.cursor_y, 23);
        assert_eq!(data.lines, 50);
        assert_eq!(data.columns, 120);
    }

    #[test]
    fn parse_pipe_data_zeros() {
        let data = parse_pipe_data_str("0,0:0:24,80").unwrap();
        assert_eq!(data.cursor_x, 0);
        assert_eq!(data.cursor_y, 0);
        assert_eq!(data.lines, 24);
        assert_eq!(data.columns, 80);
    }

    #[test]
    fn parse_pipe_data_no_scroll() {
        let data = parse_pipe_data_str("0,3:10:24,80").unwrap();
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
        assert!(parse_pipe_data_str("abc,0:0:24,80").is_err());
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
        let err = parse_pipe_data_str("0,0:0:0,80");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("KITTY_PIPE_DATA:"), "error should have standard prefix: {msg}");
        assert!(msg.contains("lines"), "error should mention lines: {msg}");
    }

    #[test]
    fn parse_pipe_data_rejects_zero_columns() {
        let err = parse_pipe_data_str("0,0:0:24,0");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("KITTY_PIPE_DATA:"), "error should have standard prefix: {msg}");
        assert!(msg.contains("columns"), "error should mention columns: {msg}");
    }
}
