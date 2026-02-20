mod kitty;
mod output;
mod palette;
mod terminal;

use anyhow::{bail, Context, Result};
use std::env;

fn main() {
    if let Err(e) = run() {
        eprintln!("kakoune-scrollback: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // 1. Re-entry prevention
    if env::var("KAKOUNE_SCROLLBACK").is_ok() {
        bail!("Already inside kakoune-scrollback");
    }

    // 2. Parse environment variables
    let pipe_data = kitty::parse_pipe_data()?;
    let window_id = kitty::window_id()?;

    // 3. Read stdin + vt100 processing (cursor position calculated in Rust)
    let screen = terminal::process_stdin(&pipe_data)?;

    // 4. Create temporary directory
    let tmp_dir = tempfile::Builder::new()
        .prefix("ksb-")
        .tempdir()
        .context("failed to create temporary directory")?;
    let text_path = tmp_dir.path().join("text.txt");
    let ranges_path = tmp_dir.path().join("ranges.kak");
    let init_path = tmp_dir.path().join("init.kak");

    // 5. Write output files
    output::write_text(&text_path, &screen)?;
    output::write_ranges(&ranges_path, &screen)?;
    output::write_init_kak(&init_path, &screen, &window_id, tmp_dir.path(), &ranges_path)?;

    // 6. Disable TempDir auto-deletion (Kakoune hook will clean up)
    let tmp_path = tmp_dir.keep();

    // 7. exec kak to replace this process
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("kak")
        .env("KAKOUNE_SCROLLBACK", "1")
        .arg("-e")
        .arg(format!("source '{}'", init_path.display()))
        .arg(&text_path)
        .exec();

    // exec failed — clean up and report
    let _ = std::fs::remove_dir_all(&tmp_path);
    Err(err).context("failed to exec kak")
}
