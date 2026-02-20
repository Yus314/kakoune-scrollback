use anyhow::{bail, Context, Result};

pub struct KittyPipeData {
    pub scrolled_by: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub lines: u16,
    pub columns: u16,
}

/// Pure function: parse from string (separated for testability)
/// Format: "scrolled_by:cursor_x,cursor_y:lines,columns"
pub fn parse_pipe_data_str(s: &str) -> Result<KittyPipeData> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        bail!("KITTY_PIPE_DATA: expected 3 colon-separated parts, got {}", parts.len());
    }

    let first: Vec<&str> = parts[0].split(',').collect();
    let (scrolled_by_str, cursor_x_str) = match first.len() {
        1 => (first[0], "0"),
        2 => (first[0], first[1]),
        _ => bail!("KITTY_PIPE_DATA: invalid first part '{}'", parts[0]),
    };

    let cursor_y: usize = parts[1].split(',').next()
        .context("KITTY_PIPE_DATA: missing cursor_y")?
        .parse().context("KITTY_PIPE_DATA: invalid cursor_y")?;

    let last: Vec<&str> = parts[2].split(',').collect();
    if last.len() != 2 {
        bail!("KITTY_PIPE_DATA: expected 'lines,columns' in third part, got '{}'", parts[2]);
    }

    Ok(KittyPipeData {
        scrolled_by: scrolled_by_str.parse().context("KITTY_PIPE_DATA: invalid scrolled_by")?,
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

/// Get KITTY_WINDOW_ID environment variable
pub fn window_id() -> Result<String> {
    std::env::var("KITTY_WINDOW_ID")
        .context("KITTY_WINDOW_ID environment variable not set")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pipe_data_valid() {
        let data = parse_pipe_data_str("42,5:23:50,120").unwrap();
        assert_eq!(data.scrolled_by, 42);
        assert_eq!(data.cursor_x, 5);
        assert_eq!(data.cursor_y, 23);
        assert_eq!(data.lines, 50);
        assert_eq!(data.columns, 120);
    }

    #[test]
    fn parse_pipe_data_zeros() {
        let data = parse_pipe_data_str("0,0:0:24,80").unwrap();
        assert_eq!(data.scrolled_by, 0);
        assert_eq!(data.cursor_x, 0);
        assert_eq!(data.cursor_y, 0);
        assert_eq!(data.lines, 24);
        assert_eq!(data.columns, 80);
    }

    #[test]
    fn parse_pipe_data_no_scroll() {
        // When not scrolled, scrolled_by might be just a number without comma
        let data = parse_pipe_data_str("0,3:10:24,80").unwrap();
        assert_eq!(data.scrolled_by, 0);
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
}
