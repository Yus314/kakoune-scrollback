use anyhow::{bail, Context, Result};

pub(crate) const CONF_SNIPPET: &str = include_str!("../rc/tmux.conf");

/// Print recommended tmux.conf configuration to stdout.
pub(crate) fn generate_conf() {
    print!("{}", CONF_SNIPPET);
}

/// Parse tmux version string and verify >= 3.3.
/// Accepts formats like "tmux 3.3", "tmux 3.3a", "3.4", etc.
/// Unparseable components default to 0 (e.g. "not-a-version" → 0.0 → Err).
fn parse_version(stdout: &str) -> Result<()> {
    let version_str = stdout.trim();
    let stripped = version_str.strip_prefix("tmux ").unwrap_or(version_str);
    let mut parts = stripped.split('.');
    let major: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minor_str = parts.next().unwrap_or("0");
    let minor: u32 = minor_str
        .trim_end_matches(|c: char| c.is_alphabetic())
        .parse()
        .unwrap_or(0);
    if (major, minor) < (3, 3) {
        bail!(
            "tmux 3.3 or later is required (found '{stripped}'). \
             display-popup -b, -e, -T were added in tmux 3.3."
        );
    }
    Ok(())
}

/// Check that tmux >= 3.3 is available (needed for display-popup -b, -e, -T).
pub(crate) fn check_version() -> Result<()> {
    let output = std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .context("failed to run 'tmux -V' — is tmux installed?")?;
    let version_str = String::from_utf8_lossy(&output.stdout);
    parse_version(&version_str)
}

/// Insert CR before every bare LF so the vt100 parser resets the column.
/// `tmux capture-pane -e -p` uses LF-only line endings.
pub(crate) fn normalize_capture(data: &mut Vec<u8>) {
    // Pass 1: count bare LFs
    let bare_lf_count = data
        .iter()
        .enumerate()
        .filter(|&(i, &b)| b == b'\n' && (i == 0 || data[i - 1] != b'\r'))
        .count();
    if bare_lf_count == 0 {
        return;
    }

    // Pass 2: expand in-place from the end
    let old_len = data.len();
    let new_len = old_len + bare_lf_count;
    data.resize(new_len, 0);
    let mut w = new_len;
    let mut r = old_len;
    while r > 0 {
        r -= 1;
        w -= 1;
        data[w] = data[r];
        if data[r] == b'\n' && (r == 0 || data[r - 1] != b'\r') {
            w -= 1;
            data[w] = b'\r';
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_snippet_is_not_empty() {
        assert!(!CONF_SNIPPET.is_empty());
        assert!(CONF_SNIPPET.contains("bind-key"));
        assert!(CONF_SNIPPET.contains("kakoune-scrollback --tmux-pane"));
        assert!(CONF_SNIPPET.contains("SCROLLBACK_PIPE_DATA"));
        assert!(CONF_SNIPPET.contains("capture-pane"));
    }

    #[test]
    fn normalize_bare_lf_to_crlf() {
        let mut data = b"line1\nline2\nline3\n".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"line1\r\nline2\r\nline3\r\n");
    }

    #[test]
    fn normalize_preserves_existing_crlf() {
        let mut data = b"line1\r\nline2\r\n".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"line1\r\nline2\r\n");
    }

    #[test]
    fn normalize_mixed_lf_and_crlf() {
        let mut data = b"line1\nline2\r\nline3\n".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"line1\r\nline2\r\nline3\r\n");
    }

    #[test]
    fn normalize_empty_input() {
        let mut data = Vec::new();
        normalize_capture(&mut data);
        assert!(data.is_empty());
    }

    #[test]
    fn normalize_leading_lf() {
        let mut data = b"\nfoo".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"\r\nfoo");
    }

    #[test]
    fn normalize_sgr_with_lf() {
        let mut data = b"\x1b[31mRed\x1b[0m\nNext line\n".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"\x1b[31mRed\x1b[0m\r\nNext line\r\n");
    }

    #[test]
    fn normalize_consecutive_lf() {
        let mut data = b"A\n\n\nB\n".to_vec();
        normalize_capture(&mut data);
        assert_eq!(data, b"A\r\n\r\n\r\nB\r\n");
    }

    // --- parse_version tests ---

    #[test]
    fn parse_version_3_3_ok() {
        assert!(parse_version("tmux 3.3").is_ok());
    }

    #[test]
    fn parse_version_3_3a_ok() {
        assert!(parse_version("tmux 3.3a").is_ok());
    }

    #[test]
    fn parse_version_3_4_ok() {
        assert!(parse_version("tmux 3.4").is_ok());
    }

    #[test]
    fn parse_version_4_0_ok() {
        assert!(parse_version("tmux 4.0").is_ok());
    }

    #[test]
    fn parse_version_3_2_too_old() {
        let err = parse_version("tmux 3.2").unwrap_err();
        assert!(err.to_string().contains("3.3 or later"));
    }

    #[test]
    fn parse_version_2_9a_too_old() {
        assert!(parse_version("tmux 2.9a").is_err());
    }

    #[test]
    fn parse_version_no_prefix() {
        assert!(parse_version("3.4").is_ok());
    }

    #[test]
    fn parse_version_empty() {
        assert!(parse_version("").is_err());
    }

    #[test]
    fn parse_version_garbage() {
        assert!(parse_version("not-a-version").is_err());
    }

    #[test]
    fn parse_version_trailing_newline() {
        assert!(parse_version("tmux 3.3\n").is_ok());
    }
}
