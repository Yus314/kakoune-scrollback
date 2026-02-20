# kakoune-scrollback

Kitty scrollback viewer for Kakoune.

## Features

- Full ANSI color and attribute rendering via Kakoune's range-specs
- View entire scrollback or last command output
- Yank selection to clipboard (OSC 52), paste or execute in Kitty
- Compose window for editing before paste/execute
- Cursor position restoration

## Requirements

- [Kitty](https://sw.kovidgoyal.net/kitty/) terminal emulator (`allow_remote_control` and `listen_on` must be enabled)
- [Kakoune](https://kakoune.org/)
- Rust toolchain (for building from source)

## Installation

```sh
make install
# or specify a prefix
make install PREFIX=~/.local
```

With Nix:

```sh
nix build
# or
nix profile install
```

## Setup

Run `:kakoune-scrollback-generate-kitty-conf` inside Kakoune to print the recommended configuration, or add the following to your `kitty.conf` manually:

```
allow_remote_control yes
listen_on unix:/tmp/kitty

map ctrl+shift+h launch --type=overlay \
    --stdin-source=@screen_scrollback \
    --stdin-add-formatting \
    --stdin-add-line-wrap-markers \
    kakoune-scrollback @active-kitty-window-id

map ctrl+shift+g launch --type=overlay \
    --stdin-source=@last_cmd_output \
    --stdin-add-formatting \
    kakoune-scrollback @active-kitty-window-id
```

## Usage

### Scrollback buffer

| Key | Action |
|-----|--------|
| `q` | Quit |
| `y` | Yank selection to clipboard (OSC 52) |
| `<ret>` | Paste selection into Kitty |
| `!` | Paste and execute selection in Kitty |
| `e` | Open compose window |
| `?` | Show help |

### Compose window

| Key | Action |
|-----|--------|
| `<a-s>` | Submit (paste into Kitty) |
| `<a-ret>` | Submit and execute |
| `<esc>` | Cancel |

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `scrollback_max_lines` | `5000` | Maximum number of scrollback lines to process |
