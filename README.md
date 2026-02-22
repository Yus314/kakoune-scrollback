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

The maximum number of scrollback lines to process can be set via the
`KAKOUNE_SCROLLBACK_MAX_LINES` environment variable (default: `200000`).
To change it, add `--env KAKOUNE_SCROLLBACK_MAX_LINES=5000` to the `launch`
command in your `kitty.conf`.

## Acknowledgments

- [kitty-scrollback.nvim](https://github.com/mikesmithgh/kitty-scrollback.nvim) — Kitty scrollback viewer for Neovim. This project was inspired by kitty-scrollback.nvim.
