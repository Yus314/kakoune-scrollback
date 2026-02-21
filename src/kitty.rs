use anyhow::{bail, Context, Result};

use crate::palette;

pub struct KittyPipeData {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub lines: u16,
    pub columns: u16,
}

/// Pure function: parse from string (separated for testability)
/// Format: "scrolled_by[,cursor_x]:cursor_y:lines,columns"
pub fn parse_pipe_data_str(s: &str) -> Result<KittyPipeData> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        bail!("KITTY_PIPE_DATA: expected 3 colon-separated parts, got {}", parts.len());
    }

    let first: Vec<&str> = parts[0].split(',').collect();
    let (_scrolled_by_str, cursor_x_str) = match first.len() {
        1 => (first[0], "0"),
        2 => (first[0], first[1]),
        _ => bail!("KITTY_PIPE_DATA: invalid first part '{}'", parts[0]),
    };

    // Validate scrolled_by is numeric even though we don't use the value
    let _scrolled_by: usize = _scrolled_by_str.parse().context("KITTY_PIPE_DATA: invalid scrolled_by")?;

    let cursor_y: usize = parts[1].split(',').next()
        .context("KITTY_PIPE_DATA: missing cursor_y")?
        .parse().context("KITTY_PIPE_DATA: invalid cursor_y")?;

    let last: Vec<&str> = parts[2].split(',').collect();
    if last.len() != 2 {
        bail!("KITTY_PIPE_DATA: expected 'lines,columns' in third part, got '{}'", parts[2]);
    }

    Ok(KittyPipeData {
        cursor_x: cursor_x_str.parse().context("KITTY_PIPE_DATA: invalid cursor_x")?,
        cursor_y,
        lines: last[0].parse().context("KITTY_PIPE_DATA: invalid lines")?,
        columns: last[1].parse().context("KITTY_PIPE_DATA: invalid columns")?,
    })
}

/// Read KITTY_PIPE_DATA environment variable and delegate to parse_pipe_data_str
pub fn parse_pipe_data() -> Result<KittyPipeData> {
    let val = std::env::var("KITTY_PIPE_DATA")
        .context("KITTY_PIPE_DATA environment variable not set")?;
    parse_pipe_data_str(&val)
}

/// Pure function: validate and parse a kitty window ID string (separated for testability)
pub fn parse_window_id(s: &str) -> Result<String> {
    let id: u32 = s.parse()
        .with_context(|| format!("invalid kitty window ID '{s}' — expected a number (check your kitty.conf)"))?;
    if id == 0 {
        bail!("invalid kitty window ID '0' — window IDs start at 1");
    }
    Ok(id.to_string())
}

/// Read the target window ID from the first CLI argument and delegate to parse_window_id
pub fn window_id() -> Result<String> {
    let val = std::env::args()
        .nth(1)
        .context("missing target window ID argument (update your kitty.conf — see README)")?;
    parse_window_id(&val)
}

/// Query the running Kitty instance for its color palette.
/// Falls back to DEFAULT_PALETTE if the command fails.
pub fn get_palette(window_id: &str) -> [u8; 48] {
    let output = std::process::Command::new("kitty")
        .args(["@", "get-colors", "--match", &format!("id:{window_id}")])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            palette::parse_kitty_colors(&text)
        }
        _ => palette::DEFAULT_PALETTE,
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
        assert_eq!(parse_window_id("42").unwrap(), "42");
        assert_eq!(parse_window_id("1").unwrap(), "1");
    }

    #[test]
    fn parse_window_id_normalizes() {
        assert_eq!(parse_window_id("042").unwrap(), "42");
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
}
