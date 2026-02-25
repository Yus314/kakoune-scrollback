mod kitty;
mod output;
mod palette;
mod terminal;
mod tmux;

use anyhow::{bail, Context, Result};
use std::env;
use std::fmt;

/// Identifies the target terminal (Kitty window or tmux pane).
pub enum TargetId {
    Kitty(kitty::WindowId),
    Tmux(String), // tmux pane ID like "%5"
}

impl TargetId {
    /// Returns "kitty" or "tmux" for the Kakoune `scrollback_backend` option.
    pub fn backend_name(&self) -> &'static str {
        match self {
            TargetId::Kitty(_) => "kitty",
            TargetId::Tmux(_) => "tmux",
        }
    }
}

impl fmt::Display for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetId::Kitty(wid) => write!(f, "kitty:{wid}"),
            TargetId::Tmux(pane) => write!(f, "tmux:{pane}"),
        }
    }
}

enum CliAction {
    ShowVersion,
    ShowHelp,
    RunKitty { window_id_arg: String },
    RunTmux { pane_id: String },
    GenerateTmuxConf,
}

fn parse_args(args: &[String]) -> Result<CliAction, String> {
    match args.get(1).map(String::as_str) {
        None => Err("missing required argument: <WINDOW_ID> or --tmux-pane <PANE_ID>".into()),
        Some("-h" | "--help") => Ok(CliAction::ShowHelp),
        Some("-V" | "--version") => Ok(CliAction::ShowVersion),
        Some("--generate-tmux-conf") => Ok(CliAction::GenerateTmuxConf),
        Some("--tmux-pane") => match args.get(2).map(String::as_str) {
            Some(pane_id) if !pane_id.is_empty() => Ok(CliAction::RunTmux {
                pane_id: pane_id.to_string(),
            }),
            _ => Err("--tmux-pane requires a pane ID argument".into()),
        },
        Some(arg) if arg.starts_with('-') => Err(format!("unexpected argument '{arg}'")),
        Some(arg) => Ok(CliAction::RunKitty {
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
Terminal scrollback viewer for Kakoune (Kitty / tmux)

USAGE:
    kakoune-scrollback <WINDOW_ID>           Kitty mode
    kakoune-scrollback --tmux-pane <PANE_ID> tmux mode
    kakoune-scrollback --generate-tmux-conf  Print tmux.conf snippet

ARGS:
    <WINDOW_ID>    Target Kitty window ID (Kitty mode)

OPTIONS:
    --tmux-pane <PANE_ID>  Target tmux pane ID (tmux mode, requires tmux 3.3+)
    --generate-tmux-conf   Print recommended tmux.conf configuration
    -h, --help             Print this help message
    -V, --version          Print version information

ENVIRONMENT:
    KITTY_PIPE_DATA                Set automatically by Kitty
    SCROLLBACK_PIPE_DATA           Set by tmux keybinding (same format)
    KAKOUNE_SCROLLBACK_MAX_LINES   Max lines to process (default: 200000)

See README for setup instructions.
",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match parse_args(&args) {
        Ok(CliAction::ShowVersion) => print_version(),
        Ok(CliAction::ShowHelp) => print_help(),
        Ok(CliAction::GenerateTmuxConf) => tmux::generate_conf(),
        Ok(CliAction::RunKitty { window_id_arg }) => {
            if let Err(e) = run_kitty(&window_id_arg) {
                eprintln!("kakoune-scrollback: {e:#}");
                eprintln!("\nPress Enter to close.");
                wait_for_keypress();
                std::process::exit(1);
            }
        }
        Ok(CliAction::RunTmux { pane_id }) => {
            if let Err(e) = run_tmux(&pane_id) {
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

fn process_screen(
    pipe_data: &kitty::PipeData,
    palette: &[u8; 48],
    stdin_data: &[u8],
    max_scrollback_lines: usize,
) -> terminal::ProcessedScreen {
    terminal::process_bytes(pipe_data, stdin_data, palette, max_scrollback_lines)
}

fn materialize(
    screen: &terminal::ProcessedScreen,
    target: &TargetId,
) -> Result<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    let tmp_dir = tempfile::Builder::new()
        .prefix("ksb-")
        .tempdir()
        .context("failed to create temporary directory")?;
    let text_path = tmp_dir.path().join("text.txt");
    let ranges_path = tmp_dir.path().join("ranges.kak");
    let init_path = tmp_dir.path().join("init.kak");

    output::write_text(&text_path, screen)?;
    output::write_ranges(&ranges_path, screen)?;
    output::write_init_kak(&init_path, screen, target, tmp_dir.path(), &ranges_path)?;

    Ok((tmp_dir, text_path, init_path))
}

fn run_core(
    pipe_data: &kitty::PipeData,
    target: &TargetId,
    palette: &[u8; 48],
    stdin_data: &[u8],
    max_scrollback_lines: usize,
) -> Result<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    let screen = process_screen(pipe_data, palette, stdin_data, max_scrollback_lines);
    materialize(&screen, target)
}

fn read_input_bounded<R: std::io::Read>(reader: R, max_bytes: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    reader.take(max_bytes + 1).read_to_end(&mut data)?;
    anyhow::ensure!(
        data.len() as u64 <= max_bytes,
        "scrollback input exceeds {max_bytes} bytes, aborting"
    );
    Ok(data)
}

fn parse_max_lines(value: &str) -> Option<usize> {
    value.trim().parse().ok()
}

fn resolve_max_scrollback_lines() -> usize {
    match env::var("KAKOUNE_SCROLLBACK_MAX_LINES") {
        Err(env::VarError::NotPresent) => terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        Err(env::VarError::NotUnicode(_)) => {
            eprintln!(
                "warning: KAKOUNE_SCROLLBACK_MAX_LINES contains invalid UTF-8, \
                 using default ({}).",
                terminal::DEFAULT_MAX_SCROLLBACK_LINES
            );
            terminal::DEFAULT_MAX_SCROLLBACK_LINES
        }
        Ok(val) => match parse_max_lines(&val) {
            Some(n) => n,
            None => {
                eprintln!(
                    "warning: invalid KAKOUNE_SCROLLBACK_MAX_LINES value {:?}, \
                     using default ({}).",
                    val,
                    terminal::DEFAULT_MAX_SCROLLBACK_LINES
                );
                terminal::DEFAULT_MAX_SCROLLBACK_LINES
            }
        },
    }
}

const MAX_STDIN_BYTES: u64 = 512 * 1024 * 1024; // 512 MB

fn run_kitty(window_id_arg: &str) -> Result<()> {
    check_reentry(env::var("KAKOUNE_SCROLLBACK").ok().as_deref())?;

    let pipe_data = kitty::parse_pipe_data()?;
    let window_id = kitty::parse_window_id(window_id_arg)?;
    let palette = kitty::get_palette(window_id);
    let stdin_data = read_input_bounded(std::io::stdin(), MAX_STDIN_BYTES)?;

    let max_scrollback_lines = resolve_max_scrollback_lines();

    let target = TargetId::Kitty(window_id);
    let (tmp_dir, text_path, init_path) = run_core(
        &pipe_data,
        &target,
        &palette,
        &stdin_data,
        max_scrollback_lines,
    )?;

    exec_kak(tmp_dir, &text_path, &init_path)
}

fn run_tmux(pane_id: &str) -> Result<()> {
    check_reentry(env::var("KAKOUNE_SCROLLBACK").ok().as_deref())?;
    tmux::check_version()?;

    let pipe_data_str = env::var("SCROLLBACK_PIPE_DATA")
        .context("SCROLLBACK_PIPE_DATA not set (should be set by tmux keybinding)")?;
    let pipe_data = kitty::parse_pipe_data_str(&pipe_data_str)?;

    let palette = palette::DEFAULT_PALETTE;

    let mut stdin_data = read_input_bounded(std::io::stdin(), MAX_STDIN_BYTES).context(
        "Set KAKOUNE_SCROLLBACK_MAX_LINES to limit processing, \
                  or reduce scrollback history in tmux (set-option -g history-limit).",
    )?;

    tmux::normalize_capture(&mut stdin_data);

    let max_scrollback_lines = resolve_max_scrollback_lines();

    let target = TargetId::Tmux(pane_id.to_string());
    let (tmp_dir, text_path, init_path) = run_core(
        &pipe_data,
        &target,
        &palette,
        &stdin_data,
        max_scrollback_lines,
    )?;

    exec_kak(tmp_dir, &text_path, &init_path)
}

fn build_kak_command(
    text_path: &std::path::Path,
    init_path: &std::path::Path,
) -> std::process::Command {
    let init_path_escaped = output::escape_kak_single_quote(&init_path.display().to_string());
    let mut cmd = std::process::Command::new("kak");
    cmd.env("KAKOUNE_SCROLLBACK", "1")
        .arg("-e")
        .arg(format!("source '{init_path_escaped}'"))
        .arg(text_path);
    cmd
}

/// Replace the current process with kak, sourcing the generated init.kak.
fn exec_kak(
    tmp_dir: tempfile::TempDir,
    text_path: &std::path::Path,
    init_path: &std::path::Path,
) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let tmp_path = tmp_dir.keep();
    let err = build_kak_command(text_path, init_path).exec();

    let _ = std::fs::remove_dir_all(&tmp_path);
    Err(err).context("failed to exec kak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kitty::PipeData;

    fn kitty_target(s: &str) -> TargetId {
        TargetId::Kitty(kitty::parse_window_id(s).unwrap())
    }

    fn default_pipe_data() -> PipeData {
        PipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        }
    }

    /// Render pipeline outputs in-memory (no filesystem needed).
    fn process_and_render(
        pipe_data: &PipeData,
        target: &TargetId,
        palette: &[u8; 48],
        stdin_data: &[u8],
    ) -> (String, String, String) {
        let screen = terminal::process_bytes(
            pipe_data,
            stdin_data,
            palette,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let mut text_buf = Vec::new();
        output::write_text_to(&mut text_buf, &screen).unwrap();
        let text = String::from_utf8(text_buf).unwrap();

        let mut ranges_buf = Vec::new();
        output::write_ranges_to(&mut ranges_buf, &screen).unwrap();
        let ranges = String::from_utf8(ranges_buf).unwrap();

        let init = output::render_init_kak(
            &screen,
            target,
            std::path::Path::new("/test/ksb-fake"),
            std::path::Path::new("/test/ksb-fake/ranges.kak"),
        )
        .unwrap();
        (text, ranges, init)
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

    // --- build_kak_command ---

    #[test]
    fn build_kak_command_program() {
        let cmd = build_kak_command(
            std::path::Path::new("/tmp/text.txt"),
            std::path::Path::new("/tmp/init.kak"),
        );
        assert_eq!(cmd.get_program(), "kak");
    }

    #[test]
    fn build_kak_command_args() {
        let cmd = build_kak_command(
            std::path::Path::new("/tmp/text.txt"),
            std::path::Path::new("/tmp/init.kak"),
        );
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "-e");
        assert_eq!(args[1], "source '/tmp/init.kak'");
        assert_eq!(args[2], "/tmp/text.txt");
    }

    #[test]
    fn build_kak_command_env() {
        let cmd = build_kak_command(
            std::path::Path::new("/tmp/text.txt"),
            std::path::Path::new("/tmp/init.kak"),
        );
        let envs: Vec<(&std::ffi::OsStr, Option<&std::ffi::OsStr>)> = cmd.get_envs().collect();
        assert!(
            envs.iter()
                .any(|(k, v)| k == &"KAKOUNE_SCROLLBACK" && v == &Some(std::ffi::OsStr::new("1"))),
            "KAKOUNE_SCROLLBACK=1 should be set, got: {envs:?}"
        );
    }

    #[test]
    fn build_kak_command_init_path_with_quote() {
        let cmd = build_kak_command(
            std::path::Path::new("/tmp/text.txt"),
            std::path::Path::new("/tmp/it's/init.kak"),
        );
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert_eq!(args[1], "source '/tmp/it''s/init.kak'");
    }

    #[test]
    fn build_kak_command_path_with_space() {
        let cmd = build_kak_command(
            std::path::Path::new("/tmp/my dir/text.txt"),
            std::path::Path::new("/tmp/my dir/init.kak"),
        );
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        // Space in path should not split the argument
        assert_eq!(args.len(), 3);
        assert_eq!(args[2], "/tmp/my dir/text.txt");
    }

    // --- parse_max_lines ---

    #[test]
    fn parse_max_lines_valid() {
        assert_eq!(parse_max_lines("100000"), Some(100000));
        assert_eq!(parse_max_lines("0"), Some(0));
        assert_eq!(parse_max_lines("1"), Some(1));
        assert_eq!(parse_max_lines(" 100 "), Some(100));
        assert_eq!(parse_max_lines("100\n"), Some(100));
    }

    #[test]
    fn parse_max_lines_invalid() {
        assert_eq!(parse_max_lines(""), None);
        assert_eq!(parse_max_lines("abc"), None);
        assert_eq!(parse_max_lines("-1"), None);
        assert_eq!(parse_max_lines("3.14"), None);
        assert_eq!(parse_max_lines("100_000"), None);
    }

    #[test]
    fn parse_max_lines_overflow() {
        // usize::MAX + 1 as a string
        let overflow = format!("{}0", usize::MAX);
        assert_eq!(parse_max_lines(&overflow), None);
    }

    // --- read_input_bounded ---

    #[test]
    fn read_input_bounded_within_limit() {
        let data = b"hello world";
        let result = read_input_bounded(std::io::Cursor::new(data), 100).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn read_input_bounded_exact_limit() {
        let data = vec![0u8; 100];
        let result = read_input_bounded(std::io::Cursor::new(&data), 100).unwrap();
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn read_input_bounded_exceeds_limit() {
        let data = vec![0u8; 101];
        let err = read_input_bounded(std::io::Cursor::new(&data), 100);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("exceeds"),
            "error should mention 'exceeds', got: {msg}"
        );
    }

    #[test]
    fn read_input_bounded_empty() {
        let result = read_input_bounded(std::io::Cursor::new(b""), 100).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn read_input_bounded_zero_limit() {
        // Empty input with zero limit should succeed
        let result = read_input_bounded(std::io::Cursor::new(b""), 0).unwrap();
        assert!(result.is_empty());
        // Any data with zero limit should fail
        let err = read_input_bounded(std::io::Cursor::new(b"x"), 0);
        assert!(err.is_err());
    }

    #[test]
    fn read_input_bounded_large_input() {
        let data = vec![0u8; 1024];
        let err = read_input_bounded(std::io::Cursor::new(&data), 512);
        assert!(err.is_err());
    }

    // --- 2. tmpdir ---

    #[test]
    fn pipeline_creates_expected_files() {
        let pd = default_pipe_data();
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, _, _) = materialize(&screen, &kitty_target("1")).unwrap();
        let path = tmp_dir.path().to_path_buf();
        assert!(path.exists());
        drop(tmp_dir);
        assert!(
            !path.exists(),
            "TempDir should be removed on drop (materialize must not call keep())"
        );
    }

    // --- 3. end-to-end pipeline ---

    #[test]
    fn pipeline_plain_text_e2e() {
        let pd = PipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let (text, ranges, init) = process_and_render(
            &pd,
            &kitty_target("42"),
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
        let pd = PipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let input = b"\x1b[31mRed\x1b[0m Normal";
        let (text, ranges, init) =
            process_and_render(&pd, &kitty_target("1"), &palette::DEFAULT_PALETTE, input);

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
        let pd = PipeData {
            cursor_x: 0,
            cursor_y: 5,
            lines: 10,
            columns: 80,
        };
        let (_text, _ranges, init) =
            process_and_render(&pd, &kitty_target("1"), &palette::DEFAULT_PALETTE, &input);

        // total_sb = 21, cursor_output_line = 21 + 5 + 1 = 27
        assert!(init.contains("select 27.1,27.1"));
        // viewport_top_line = 21 + 1 = 22, should use vt
        assert!(
            init.contains("select 22.1,22.1"),
            "should set viewport top line to 22, got:\n{init}"
        );
        assert!(
            init.contains("execute-keys vt"),
            "should use vt for viewport positioning, got:\n{init}"
        );
        assert!(
            !init.contains("execute-keys vb"),
            "should not contain vb, got:\n{init}"
        );
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
        let (_text, ranges, _init) =
            process_and_render(&pd, &kitty_target("1"), &custom_palette, input);

        // SGR 31 = color index 1 → should resolve to our custom #AABBCC
        assert!(
            ranges.contains("rgb:AABBCC"),
            "Custom palette color should appear in ranges, got: {ranges}"
        );
    }

    #[test]
    fn pipeline_empty_input() {
        let pd = default_pipe_data();
        let (text, ranges, init) =
            process_and_render(&pd, &kitty_target("1"), &palette::DEFAULT_PALETTE, b"");

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
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, _, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let (_text, _ranges, init) = process_and_render(
            &pd,
            &kitty_target("999"),
            &palette::DEFAULT_PALETTE,
            b"test",
        );

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
        let pd = PipeData {
            cursor_x: 6,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let (_text, _ranges, init) =
            process_and_render(&pd, &kitty_target("1"), &palette::DEFAULT_PALETTE, input);

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
        let pd = PipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello\r\nworld",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_colored_text() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = b"\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m";
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_wide_chars() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = "日本語テスト".as_bytes();
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let pd = PipeData {
            cursor_x: 0,
            cursor_y: 5,
            lines: 24,
            columns: 80,
        };
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            &input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            &input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_validates_empty_input() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    #[test]
    fn kak_scrollback_colors_populated() {
        if !kak_available() {
            return;
        }
        let pd = default_pipe_data();
        let input = b"\x1b[31mRed text here\x1b[0m";
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let pd = PipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello\r\nworld",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
        let pd = PipeData {
            cursor_x: 6,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            input,
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &kitty_target("1")).unwrap();
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
            Ok(CliAction::RunKitty { window_id_arg }) if window_id_arg == "42"
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

    // --- tmux CLI arg parsing ---

    #[test]
    fn parse_args_tmux_pane() {
        let args = vec!["ksb".into(), "--tmux-pane".into(), "%5".into()];
        assert!(matches!(
            parse_args(&args),
            Ok(CliAction::RunTmux { pane_id }) if pane_id == "%5"
        ));
    }

    #[test]
    fn parse_args_tmux_pane_missing_id() {
        let args = vec!["ksb".into(), "--tmux-pane".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_tmux_pane_empty_id() {
        let args = vec!["ksb".into(), "--tmux-pane".into(), "".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_generate_tmux_conf() {
        let args = vec!["ksb".into(), "--generate-tmux-conf".into()];
        assert!(matches!(parse_args(&args), Ok(CliAction::GenerateTmuxConf)));
    }

    // --- TargetId ---

    #[test]
    fn target_id_backend_name_kitty() {
        let t = kitty_target("1");
        assert_eq!(t.backend_name(), "kitty");
    }

    #[test]
    fn target_id_backend_name_tmux() {
        let t = TargetId::Tmux("%5".to_string());
        assert_eq!(t.backend_name(), "tmux");
    }

    #[test]
    fn target_id_display() {
        let t = kitty_target("42");
        assert_eq!(t.to_string(), "kitty:42");
        let t = TargetId::Tmux("%5".to_string());
        assert_eq!(t.to_string(), "tmux:%5");
    }

    // --- run_core with tmux target ---

    #[test]
    fn pipeline_tmux_target_e2e() {
        let pd = PipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let target = TargetId::Tmux("%5".to_string());
        let (text, _ranges, init) = process_and_render(
            &pd,
            &target,
            &palette::DEFAULT_PALETTE,
            b"line one\r\nline two",
        );

        assert_eq!(text, "line one\nline two\n");
        assert!(init.contains("scrollback_backend 'tmux'"));
        assert!(init.contains("scrollback_tmux_pane_id '%5'"));
        assert!(!init.contains("scrollback_kitty_window_id"));
        assert!(init.contains("select 2.4,2.4"));
    }

    #[test]
    fn pipeline_tmux_target_special_chars_in_pane_id() {
        let pd = default_pipe_data();
        let target = TargetId::Tmux("some'target".to_string());
        let (_text, _ranges, init) =
            process_and_render(&pd, &target, &palette::DEFAULT_PALETTE, b"test");

        // Single quote in pane_id should be escaped for Kakoune
        assert!(init.contains("scrollback_tmux_pane_id 'some''target'"));
    }

    // --- write_init_kak with tmux target (Kakoune integration) ---

    #[test]
    fn kak_validates_tmux_target() {
        if !kak_available() {
            return;
        }
        let pd = PipeData {
            cursor_x: 3,
            cursor_y: 1,
            lines: 24,
            columns: 80,
        };
        let target = TargetId::Tmux("%5".to_string());
        let screen = process_screen(
            &pd,
            &palette::DEFAULT_PALETTE,
            b"hello\r\nworld",
            terminal::DEFAULT_MAX_SCROLLBACK_LINES,
        );
        let (tmp_dir, text_path, init_path) = materialize(&screen, &target).unwrap();
        validate_in_kak(&text_path, &init_path, tmp_dir.path());
    }

    // --- normalize + pipeline integration tests ---

    #[test]
    fn normalize_then_pipeline_lines_separate() {
        let mut input = b"line one\nline two\n".to_vec();
        tmux::normalize_capture(&mut input);
        let pd = PipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let (text, _ranges, _init) = process_and_render(
            &pd,
            &TargetId::Tmux("%0".to_string()),
            &palette::DEFAULT_PALETTE,
            &input,
        );
        assert_eq!(text, "line one\nline two\n");
    }

    #[test]
    fn normalize_then_pipeline_colored() {
        let mut input = b"\x1b[31mRed\x1b[0m\n\x1b[32mGreen\x1b[0m\n".to_vec();
        tmux::normalize_capture(&mut input);
        let pd = default_pipe_data();
        let (text, ranges, _init) = process_and_render(
            &pd,
            &TargetId::Tmux("%0".to_string()),
            &palette::DEFAULT_PALETTE,
            &input,
        );
        assert_eq!(text, "Red\nGreen\n");
        assert!(ranges.contains("rgb:"));
    }

    /// Regression: without normalization, bare LF causes text to shift right
    /// because the vt100 parser does not reset the column on bare LF.
    #[test]
    fn bare_lf_without_normalize_shifts_text() {
        // Feed bare-LF input directly (no normalization)
        let input = b"AAA\nBBB\n";
        let pd = PipeData {
            cursor_x: 0,
            cursor_y: 0,
            lines: 24,
            columns: 80,
        };
        let (text, _ranges, _init) = process_and_render(
            &pd,
            &TargetId::Tmux("%0".to_string()),
            &palette::DEFAULT_PALETTE,
            input,
        );
        // Without CR, "BBB" starts at column 3 (after "AAA"), producing "AAABBB"
        // on the same or offset line rather than clean separate lines.
        assert_ne!(
            text, "AAA\nBBB\n",
            "bare LF without normalization should NOT produce clean line separation"
        );
    }
}
