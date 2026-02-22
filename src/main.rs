mod kitty;
mod output;
mod palette;
mod terminal;

use anyhow::{bail, Context, Result};
use std::env;

enum CliAction {
    ShowVersion,
    ShowHelp,
    Run { window_id_arg: String },
}

fn parse_args(args: &[String]) -> Result<CliAction, String> {
    match args.get(1).map(String::as_str) {
        None => Err("missing required argument: <WINDOW_ID>".into()),
        Some("-h" | "--help") => Ok(CliAction::ShowHelp),
        Some("-V" | "--version") => Ok(CliAction::ShowVersion),
        Some(arg) if arg.starts_with('-') => Err(format!("unexpected argument '{arg}'")),
        Some(arg) => Ok(CliAction::Run {
            window_id_arg: arg.to_string(),
        }),
    }
}

fn print_version() {
    println!("kakoune-scrollback {}", env!("CARGO_PKG_VERSION"));
}

fn print_help() {
    print!(
        "\
kakoune-scrollback {}
Kitty scrollback viewer for Kakoune

USAGE:
    kakoune-scrollback <WINDOW_ID>

ARGS:
    <WINDOW_ID>    Target Kitty window ID

OPTIONS:
    -h, --help       Print this help message
    -V, --version    Print version information

ENVIRONMENT:
    KITTY_PIPE_DATA                Set automatically by Kitty
    KAKOUNE_SCROLLBACK_MAX_LINES   Max lines to process (default: 200000)

This tool is invoked via Kitty's pipe mechanism. See README for setup.
",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match parse_args(&args) {
        Ok(CliAction::ShowVersion) => print_version(),
        Ok(CliAction::ShowHelp) => print_help(),
        Ok(CliAction::Run { window_id_arg }) => {
            if let Err(e) = run(&window_id_arg) {
                eprintln!("kakoune-scrollback: {e:#}");
                eprintln!("\nPress Enter to close.");
                wait_for_keypress();
                std::process::exit(1);
            }
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("\nFor more information, try '--help'.");
            std::process::exit(2);
        }
    }
}

fn wait_for_keypress() {
    use std::io::BufRead;
    if let Ok(tty) = std::fs::File::open("/dev/tty") {
        let _ = std::io::BufReader::new(tty).read_line(&mut String::new());
    }
}

fn check_reentry(env_val: Option<&str>) -> Result<()> {
    if env_val.is_some() {
        bail!("Already inside kakoune-scrollback");
    }
    Ok(())
}

fn run_core(
    pipe_data: &kitty::KittyPipeData,
    window_id: kitty::WindowId,
    palette: &[u8; 48],
    stdin_data: &[u8],
    max_scrollback_lines: usize,
) -> Result<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    let screen = terminal::process_bytes(pipe_data, stdin_data, palette, max_scrollback_lines);

    let tmp_dir = tempfile::Builder::new()
        .prefix("ksb-")
        .tempdir()
        .context("failed to create temporary directory")?;
    let text_path = tmp_dir.path().join("text.txt");
    let ranges_path = tmp_dir.path().join("ranges.kak");
    let init_path = tmp_dir.path().join("init.kak");

    output::write_text(&text_path, &screen)?;
    output::write_ranges(&ranges_path, &screen)?;
    output::write_init_kak(&init_path, &screen, window_id, tmp_dir.path(), &ranges_path)?;

    Ok((tmp_dir, text_path, init_path))
}

fn check_stdin_size(actual: u64, max: u64) -> Result<()> {
    anyhow::ensure!(
        actual <= max,
        "scrollback input exceeds {max} bytes, aborting"
    );
    Ok(())
}

fn run(window_id_arg: &str) -> Result<()> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    const MAX_STDIN_BYTES: u64 = 512 * 1024 * 1024; // 512 MB

    check_reentry(env::var("KAKOUNE_SCROLLBACK").ok().as_deref())?;

    let pipe_data = kitty::parse_pipe_data()?;
    let window_id = kitty::parse_window_id(window_id_arg)?;
    let palette = kitty::get_palette(window_id);
    let mut stdin_data = Vec::new();
    std::io::stdin()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_end(&mut stdin_data)?;
    check_stdin_size(stdin_data.len() as u64, MAX_STDIN_BYTES)?;

    let max_scrollback_lines: usize = env::var("KAKOUNE_SCROLLBACK_MAX_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(terminal::DEFAULT_MAX_SCROLLBACK_LINES);

    let (tmp_dir, text_path, init_path) = run_core(
        &pipe_data,
        window_id,
        &palette,
        &stdin_data,
        max_scrollback_lines,
    )?;

    let tmp_path = tmp_dir.keep();

    let init_path_escaped = output::escape_kak_single_quote(&init_path.display().to_string());

    let err = std::process::Command::new("kak")
        .env("KAKOUNE_SCROLLBACK", "1")
        .arg("-e")
        .arg(format!("source '{init_path_escaped}'"))
        .arg(&text_path)
        .exec();

    let _ = std::fs::remove_dir_all(&tmp_path);
    Err(err).context("failed to exec kak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kitty::{KittyPipeData, WindowId};

    fn wid(s: &str) -> WindowId {
        kitty::parse_window_id(s).unwrap()
    }

    fn default_pipe_data() -> KittyPipeData {
        KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        }
    }

    /// Run `run_core()`, read back the 3 output files, return their contents and the TempDir.
    fn run_and_read(
        pipe_data: &KittyPipeData,
        window_id: WindowId,
        palette: &[u8; 48],
        stdin_data: &[u8],
    ) -> (String, String, String, tempfile::TempDir) {
        let (tmp_dir, text_path, init_path) = run_core(
            pipe_data,
            window_id,
            palette,
            stdin_data,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let ranges_path = tmp_dir.path().join("ranges.kak");
        let text = std::fs::read_to_string(&text_path).unwrap();
        let ranges = std::fs::read_to_string(&ranges_path).unwrap();
        let init = std::fs::read_to_string(&init_path).unwrap();
        (text, ranges, init, tmp_dir)
    }

    // --- 1. check_reentry ---

    #[test]
    fn check_reentry_blocks() {
        let err = check_reentry(Some("1"));
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Already inside kakoune-scrollback"));
    }

    #[test]
    fn check_reentry_allows() {
        assert!(check_reentry(None).is_ok());
    }

    // --- check_stdin_size ---

    #[test]
    fn check_stdin_size_within_limit() {
        assert!(check_stdin_size(0, 100).is_ok());
        assert!(check_stdin_size(99, 100).is_ok());
        assert!(check_stdin_size(100, 100).is_ok()); // exactly at limit = OK
    }

    #[test]
    fn check_stdin_size_exceeds_limit() {
        assert!(check_stdin_size(101, 100).is_err());
        assert!(check_stdin_size(200, 100).is_err());
    }

    #[test]
    fn check_stdin_size_zero_max() {
        assert!(check_stdin_size(0, 0).is_ok());
        assert!(check_stdin_size(1, 0).is_err());
    }

    // --- 2. tmpdir ---

    #[test]
    fn pipeline_creates_expected_files() {
        let pd = default_pipe_data();
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let ranges_path = tmp_dir.path().join("ranges.kak");

        assert!(text_path.exists());
        assert!(init_path.exists());
        assert!(ranges_path.exists());

        // tmpdir has ksb- prefix
        let dir_name = tmp_dir.path().file_name().unwrap().to_str().unwrap();
        assert!(
            dir_name.starts_with("ksb-"),
            "tmpdir should have ksb- prefix, got: {dir_name}"
        );
    }

    #[test]
    fn pipeline_tmpdir_not_kept() {
        let pd = default_pipe_data();
        let (tmp_dir, _, _) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let path = tmp_dir.path().to_path_buf();
        assert!(path.exists());
        drop(tmp_dir);
        assert!(
            !path.exists(),
            "TempDir should be removed on drop (run_core must not call keep())"
        );
    }

    // --- 3. end-to-end pipeline ---

    #[test]
    fn pipeline_plain_text_e2e() {
        let pd = KittyPipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let (text, ranges, init, _td) = run_and_read(
            &pd,
            wid("42"),
            &palette::DEFAULT_PALETTE,
            b"line one\r\nline two",
        );

        assert_eq!(text, "line one\nline two\n");
        // No ANSI colors → ranges should be empty
        assert!(ranges.is_empty());
        // init should contain cursor position and window_id
        assert!(init.contains("select 2.4,2.4"));
        assert!(init.contains("scrollback_kitty_window_id '42'"));
    }

    #[test]
    fn pipeline_colored_e2e() {
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let input = b"\x1b[31mRed\x1b[0m Normal";
        let (text, ranges, init, _td) =
            run_and_read(&pd, wid("1"), &palette::DEFAULT_PALETTE, input);

        assert_eq!(text, "Red Normal\n");
        // ranges should contain a face for the red text
        assert!(ranges.contains("rgb:"));
        assert!(ranges.contains("set-option buffer scrollback_colors"));
        // init should have cursor select
        assert!(init.contains("select 1.1,1.1"));
    }

    #[test]
    fn pipeline_scrollback_cursor_e2e() {
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
        let (_text, _ranges, init, _td) =
            run_and_read(&pd, wid("1"), &palette::DEFAULT_PALETTE, &input);

        // total_sb = 21, cursor_output_line = 21 + 5 + 1 = 27
        assert!(init.contains("select 27.1,27.1"));
    }

    #[test]
    fn pipeline_custom_palette_e2e() {
        let mut custom_palette = palette::DEFAULT_PALETTE;
        // Override color 1 (red) to #AABBCC
        custom_palette[3] = 0xAA;
        custom_palette[4] = 0xBB;
        custom_palette[5] = 0xCC;

        let pd = default_pipe_data();
        let input = b"\x1b[31mColored\x1b[0m";
        let (_text, ranges, _init, _td) = run_and_read(&pd, wid("1"), &custom_palette, input);

        // SGR 31 = color index 1 → should resolve to our custom #AABBCC
        assert!(
            ranges.contains("rgb:AABBCC"),
            "Custom palette color should appear in ranges, got: {ranges}"
        );
    }

    #[test]
    fn pipeline_empty_input() {
        let pd = default_pipe_data();
        let (text, ranges, init, _td) = run_and_read(&pd, wid("1"), &palette::DEFAULT_PALETTE, b"");

        // Empty input → text is empty, ranges empty, but init still has structure
        assert!(text.is_empty() || text == "\n");
        assert!(ranges.is_empty());
        assert!(init.contains("scrollback_kitty_window_id"));
        assert!(init.contains("select"));
    }

    // --- 4. init.kak verification ---

    #[test]
    fn pipeline_init_references_actual_tmpdir() {
        let pd = default_pipe_data();
        let (tmp_dir, _, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let init = std::fs::read_to_string(&init_path).unwrap();
        let tmp_dir_str = tmp_dir.path().to_str().unwrap();

        // The rm -rf cleanup command should reference the actual tmpdir
        assert!(
            init.contains(&format!("rm -rf -- '{tmp_dir_str}'")),
            "init.kak should reference tmpdir in rm -rf, got: {init}"
        );
    }

    #[test]
    fn pipeline_init_window_id_propagated() {
        let pd = default_pipe_data();
        let (_text, _ranges, init, _td) =
            run_and_read(&pd, wid("999"), &palette::DEFAULT_PALETTE, b"test");

        assert!(
            init.contains("scrollback_kitty_window_id '999'"),
            "window_id should be propagated to init.kak, got: {init}"
        );
    }

    // --- 5. wide char ---

    #[test]
    fn pipeline_wide_char_cursor_e2e() {
        // "日本語" = 9 bytes (3 each), cursor at col 6 (after 3 wide chars = 6 columns)
        let input = "日本語test".as_bytes();
        let pd = KittyPipeData {
            cursor_x: 6,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let (_text, _ranges, init, _td) =
            run_and_read(&pd, wid("1"), &palette::DEFAULT_PALETTE, input);

        // "日本語" = 9 bytes, cursor_x:6 lands on 't', byte offset = 9 + 1 = 10 (1-based)
        assert!(
            init.contains("select 1.10,1.10"),
            "Wide char cursor byte offset should be correct, got: {init}"
        );
    }

    // --- 6. Kakoune integration tests (require kak on PATH) ---

    fn kak_available() -> bool {
        std::process::Command::new("kak")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn plugin_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("rc")
            .join("kakoune-scrollback.kak")
    }

    fn run_kak_cmd(commands: &str, tmp_dir: &std::path::Path) -> std::process::Output {
        let runtime_dir = tmp_dir.join("kak-runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::process::Command::new("timeout")
            .args(["5", "kak", "-n", "-ui", "dummy", "-e", commands])
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("failed to run kak")
    }

    /// Source plugin + init.kak in kak, expect clean exit.
    fn validate_in_kak(
        text_path: &std::path::Path,
        init_path: &std::path::Path,
        tmp_dir: &std::path::Path,
    ) {
        let plugin = plugin_path();
        let commands = format!(
            "edit '{text}'; source '{plugin}'; source '{init}'\nquit!",
            text = output::escape_kak_single_quote(&text_path.display().to_string()),
            plugin = output::escape_kak_single_quote(&plugin.display().to_string()),
            init = output::escape_kak_single_quote(&init_path.display().to_string()),
        );
        let out = run_kak_cmd(&commands, tmp_dir);
        assert!(
            out.status.success(),
            "kak failed (exit {:?}):\nstderr: {}\nstdout: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
        );
    }

    /// Source plugin + init.kak, write %val{selection_desc} to result file, return it.
    /// Uses a separate result_dir because init.kak's ClientClose hook deletes the main tmpdir.
    fn validate_cursor_in_kak(
        text_path: &std::path::Path,
        init_path: &std::path::Path,
        tmp_dir: &std::path::Path,
    ) -> String {
        let plugin = plugin_path();
        let result_dir = tempfile::tempdir().unwrap();
        let result_path = result_dir.path().join("cursor_result");
        let commands = format!(
            "edit '{text}'; source '{plugin}'; source '{init}'\nnop %sh{{ printf '%s' \"$kak_selection_desc\" > '{result}' }}\nquit!",
            text = output::escape_kak_single_quote(&text_path.display().to_string()),
            plugin = output::escape_kak_single_quote(&plugin.display().to_string()),
            init = output::escape_kak_single_quote(&init_path.display().to_string()),
            result = output::escape_kak_single_quote(&result_path.display().to_string()),
        );
        let out = run_kak_cmd(&commands, tmp_dir);
        assert!(
            out.status.success(),
            "kak failed (exit {:?}):\nstderr: {}\nstdout: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
        );
        std::fs::read_to_string(&result_path).expect("failed to read cursor result")
    }

    #[test]
    fn kak_validates_plain_text() {
        if !kak_available() {
            return;
        }
        let pd = KittyPipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"hello\r\nworld",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_colored_text() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = b"\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m";
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_wide_chars() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = "日本語テスト".as_bytes();
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_large_scrollback() {
        if !kak_available() {
            return;
        }
        let mut input = Vec::new();
        for i in 0..100 {
            input.extend_from_slice(format!("line {i}: the quick brown fox\r\n").as_bytes());
        }
        let pd = KittyPipeData {
            cursor_x: 0,
            cursor_y: 5,
            lines: 24,
            columns: 80,
        };
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            &input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_many_color_spans() {
        if !kak_available() {
            return;
        }
        let mut input = Vec::new();
        let colors = [b"31", b"32", b"33", b"34", b"35", b"36"];
        for i in 0..50 {
            let color = colors[i % colors.len()];
            input.extend_from_slice(b"\x1b[");
            input.extend_from_slice(color);
            input.extend_from_slice(b"m");
            input.extend_from_slice(format!("colored line {i}").as_bytes());
            input.extend_from_slice(b"\x1b[0m\r\n");
        }
        let pd = default_pipe_data();
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            &input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_empty_input() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_scrollback_colors_populated() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = b"\x1b[31mRed text here\x1b[0m";
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let plugin = plugin_path();
        let result_dir = tempfile::tempdir().unwrap();
        let result_path = result_dir.path().join("colors_result");
        let commands = format!(
            "edit '{text}'; source '{plugin}'; source '{init}'\nnop %sh{{ printf '%s' \"$kak_opt_scrollback_colors\" > '{result}' }}\nquit!",
            text = output::escape_kak_single_quote(&text_path.display().to_string()),
            plugin = output::escape_kak_single_quote(&plugin.display().to_string()),
            init = output::escape_kak_single_quote(&init_path.display().to_string()),
            result = output::escape_kak_single_quote(&result_path.display().to_string()),
        );
        let out = run_kak_cmd(&commands, tmp_dir.path());
        assert!(
            out.status.success(),
            "kak failed: {}",
            String::from_utf8_lossy(&out.stderr),
        );
        let colors = std::fs::read_to_string(&result_path).expect("failed to read colors result");
        assert!(
            !colors.is_empty(),
            "scrollback_colors should be populated after sourcing colored input"
        );
    }

    #[test]
    fn kak_cursor_position_plain() {
        if !kak_available() {
            return;
        }
        let pd = KittyPipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            b"hello\r\nworld",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let desc = validate_cursor_in_kak(&text_path, &init_path, tmp_dir.path());
        assert_eq!(
            desc.trim(),
            "2.4,2.4",
            "cursor should be at line 2, byte 4 (after 'wor')"
        );
    }

    #[test]
    fn kak_cursor_position_wide_chars() {
        if !kak_available() {
            return;
        }
        let input = "日本語test".as_bytes();
        let pd = KittyPipeData {
            cursor_x: 6,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let (tmp_dir, text_path, init_path) = run_core(
            &pd,
            wid("1"),
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        )
        .unwrap();
        let desc = validate_cursor_in_kak(&text_path, &init_path, tmp_dir.path());
        assert_eq!(
            desc.trim(),
            "1.10,1.10",
            "cursor should be at byte 10 (after 9 bytes of 日本語)"
        );
    }

    // --- parse_args ---

    #[test]
    fn parse_args_help_short() {
        let args = vec!["ksb".into(), "-h".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::ShowHelp)));
    }

    #[test]
    fn parse_args_help_long() {
        let args = vec!["ksb".into(), "--help".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::ShowHelp)));
    }

    #[test]
    fn parse_args_version_short() {
        let args = vec!["ksb".into(), "-V".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::ShowVersion)));
    }

    #[test]
    fn parse_args_version_long() {
        let args = vec!["ksb".into(), "--version".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::ShowVersion)));
    }

    #[test]
    fn parse_args_window_id() {
        let args = vec!["ksb".into(), "42".into()];
        assert!(matches!(
            parse_args(&args),
            Ok(CliAction::Run { window_id_arg }) if window_id_arg == "42"
        ));
    }

    #[test]
    fn parse_args_no_args() {
        let args = vec!["ksb".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_unknown_flag() {
        let args = vec!["ksb".into(), "--foo".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_help_with_trailing() {
        let args = vec!["ksb".into(), "--help".into(), "123".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::ShowHelp)));
    }
}
