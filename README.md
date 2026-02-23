# kakoune-scrollback

Terminal scrollback viewer for Kakoune.

## Features

- Full ANSI color and attribute rendering via Kakoune's range-specs
- View entire scrollback or last command output
- Yank selection to clipboard (OSC 52), paste or execute in terminal
- Compose window for editing before paste/execute
- Cursor position restoration
- Supports both **Kitty** and **tmux** backends

## Requirements

- [Kitty](https://sw.kovidgoyal.net/kitty/) terminal emulator (`allow_remote_control` and `listen_on` must be enabled) **or** [tmux](https://github.com/tmux/tmux) 3.3+
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

### Kitty

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

### tmux

Requires **tmux 3.3** or later (`display-popup -b`, `-e`, `-T` were added in 3.3).

Generate the recommended configuration:

```sh
kakoune-scrollback --generate-tmux-conf >> ~/.tmux.conf
```

Or run `:kakoune-scrollback-generate-tmux-conf` inside Kakoune to get the snippet in a scratch buffer.

The compose window uses `display-popup` for a floating editor that keeps the scrollback visible behind it.

**Known limitation:** The tmux backend uses a fixed default color palette for ANSI colors 0-15. If your terminal theme uses custom colors, they may not match exactly. The Kitty backend queries the actual palette from Kitty.

## Usage

### Scrollback buffer

| Key | Action |
|-----|--------|
| `q` | Quit |
| `y` | Yank selection to clipboard (OSC 52) |
| `<ret>` | Paste selection into terminal |
| `!` | Paste and execute selection in terminal |
| `e` | Open compose window |
| `?` | Show help |

### Compose window

| Key | Action |
|-----|--------|
| `<a-s>` | Submit (paste into terminal) |
| `<a-ret>` | Submit and execute |
| `<esc>` | Cancel |

## Configuration

The maximum number of scrollback lines to process can be set via the
`KAKOUNE_SCROLLBACK_MAX_LINES` environment variable (default: `200000`).
To change it, add `--env KAKOUNE_SCROLLBACK_MAX_LINES=5000` to the `launch`
command in your `kitty.conf`, or set it in the tmux keybinding environment.

## Acknowledgments

- [kitty-scrollback.nvim](https://github.com/mikesmithgh/kitty-scrollback.nvim) — Kitty scrollback viewer for Neovim. This project was inspired by kitty-scrollback.nvim.
