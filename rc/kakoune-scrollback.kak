# kakoune-scrollback — Terminal scrollback viewer for Kakoune (Kitty / tmux)

# Global options (accessible from compose client)
declare-option -hidden str scrollback_backend ''           # 'kitty' or 'tmux' (set by init.kak)
declare-option -hidden str scrollback_kitty_window_id ''   # Kitty window target
declare-option -hidden str scrollback_tmux_pane_id ''      # tmux pane target (e.g. "%5")

# Buffer-local options
declare-option -hidden str scrollback_tmp_dir ''

# Paste window initial value
declare-option -hidden str scrollback_compose_init ''

# --- Keymaps ---

define-command -hidden kakoune-scrollback-setup-keymaps %{
    map buffer normal q     ':kakoune-scrollback-quit<ret>'
    map buffer normal y     ':kakoune-scrollback-yank<ret>'
    map buffer normal <ret> ':kakoune-scrollback-paste<ret>'
    map buffer normal !     ':kakoune-scrollback-execute<ret>'
    map buffer normal e     ':kakoune-scrollback-edit<ret>'
    map buffer normal ?     ':kakoune-scrollback-help<ret>'

    # User extension points (define kakoune-scrollback-user-keymaps
    # in your kakrc to override specific keys)
    try %{ kakoune-scrollback-user-keymaps }
    trigger-user-hook scrollback-keymaps-ready
}

define-command -hidden kakoune-scrollback-setup-compose-keymaps %{
    map buffer normal <a-s>   ':kakoune-scrollback-submit<ret>'
    map buffer normal <a-ret> ':kakoune-scrollback-submit-exec<ret>'
    map buffer normal <esc>   ':kakoune-scrollback-cancel<ret>'

    try %{ kakoune-scrollback-user-compose-keymaps }
    trigger-user-hook scrollback-compose-keymaps-ready
}

# --- Backend dispatch ---

define-command -hidden kakoune-scrollback-require-backend %{
    evaluate-commands %sh{
        case "$kak_opt_scrollback_backend" in
            kitty|tmux) ;;
            '') echo "fail 'scrollback_backend not set (init.kak not loaded?)'" ;;
            *)  printf "fail 'unknown scrollback_backend: %s'\n" \
                    "$(printf '%s' "$kak_opt_scrollback_backend" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)" ;;
        esac
    }
}

define-command -hidden kakoune-scrollback-send %{
    kakoune-scrollback-require-backend
    evaluate-commands "kakoune-scrollback-send-to-%opt{scrollback_backend}"
}

define-command -hidden kakoune-scrollback-execute-target %{
    kakoune-scrollback-require-backend
    evaluate-commands "kakoune-scrollback-execute-in-%opt{scrollback_backend}"
}

define-command -hidden kakoune-scrollback-open-compose %{
    kakoune-scrollback-require-backend
    evaluate-commands "kakoune-scrollback-compose-%opt{scrollback_backend}"
}

# --- Kitty send helpers ---

define-command -hidden kakoune-scrollback-send-to-kitty %{
    evaluate-commands %sh{
        err=$(printf '%s' "$kak_selection" | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --bracketed-paste=enable \
            --stdin 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'send-text failed: ${err}'"
        fi
    }
}

define-command -hidden kakoune-scrollback-execute-in-kitty %{
    evaluate-commands %sh{
        err=$(printf '\r' | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --stdin 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'send-text failed: ${err}'"
        fi
    }
}

# --- tmux send helpers ---

define-command -hidden kakoune-scrollback-send-to-tmux %{
    evaluate-commands %sh{
        pane="$kak_opt_scrollback_tmux_pane_id"
        buf="_ksb_$$"
        trap 'tmux delete-buffer -b "$buf" 2>/dev/null' EXIT

        err=$(printf '%s' "$kak_selection" | tmux load-buffer -b "$buf" - 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'load-buffer failed: ${err}'"
            exit 0
        fi
        err=$(tmux paste-buffer -b "$buf" -t "$pane" -p 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'paste-buffer failed: ${err}'"
            exit 0
        fi
    }
}

define-command -hidden kakoune-scrollback-execute-in-tmux %{
    evaluate-commands %sh{
        pane="$kak_opt_scrollback_tmux_pane_id"
        err=$(tmux send-keys -t "$pane" Enter 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'send-keys failed: ${err}'"
        fi
    }
}

# --- Core commands ---

define-command kakoune-scrollback-quit %{
    quit!
}

define-command kakoune-scrollback-yank %{
    evaluate-commands %sh{
        encoded=$(printf '%s' "$kak_selection" | base64 | tr -d '\n')
        printf '\033]52;c;%s\a' "$encoded" > /dev/tty
    }
    kakoune-scrollback-quit
}

define-command kakoune-scrollback-paste %{
    kakoune-scrollback-send
    kakoune-scrollback-quit
}

define-command kakoune-scrollback-execute %{
    kakoune-scrollback-send
    kakoune-scrollback-execute-target
    kakoune-scrollback-quit
}

define-command kakoune-scrollback-help %{
    info -title 'kakoune-scrollback' \
        'q      : quit
y      : yank selection to clipboard (OSC 52)
<ret>  : paste selection to terminal
!      : execute selection in terminal
e      : open compose window
         <a-s>   : submit (paste)
         <a-ret> : submit and execute
         <esc>   : cancel
?      : show this help
(keys can be customized — see README)'
}

# --- Compose window ---

define-command kakoune-scrollback-edit %{
    set-option global scrollback_compose_init %val{selection}

    edit -scratch *compose*

    try %{
        set-register '"' %opt{scrollback_compose_init}
        execute-keys '<a-P>'
    }

    kakoune-scrollback-open-compose
}

# Kitty backend: full-screen overlay (existing behavior)
define-command -hidden kakoune-scrollback-compose-kitty %{
    evaluate-commands %sh{
        kitty @ launch --no-response --type=overlay \
            --env KAKOUNE_SCROLLBACK=1 \
            -- kak -c "$kak_session" -e '
                buffer *compose*
                kakoune-scrollback-setup-compose-keymaps
                execute-keys gi
            '
    }
}

# tmux backend: floating popup (tmux 3.3+)
define-command -hidden kakoune-scrollback-compose-tmux %{
    evaluate-commands %sh{
        err=$(tmux display-popup -E \
            -w 80% -h 40% \
            -b rounded \
            -T ' compose ' \
            -e "KAKOUNE_SCROLLBACK=1" \
            -- kak -c "$kak_session" -e '
                buffer *compose*
                kakoune-scrollback-setup-compose-keymaps
                execute-keys gi
            ' 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$err" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'compose popup failed: ${err}'"
        fi
    }
}

define-command -hidden kakoune-scrollback-submit %{
    execute-keys '%'
    kakoune-scrollback-send
    delete-buffer *compose*
    quit
}

define-command -hidden kakoune-scrollback-submit-exec %{
    execute-keys '%'
    kakoune-scrollback-send
    kakoune-scrollback-execute-target
    delete-buffer *compose*
    quit
}

define-command -hidden kakoune-scrollback-cancel %{
    delete-buffer *compose*
    quit
}

# --- Configuration helpers ---

define-command kakoune-scrollback-generate-kitty-conf %{
    echo -markup '{Information}Add the following to your kitty.conf:{Default}

# kakoune-scrollback
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
    kakoune-scrollback @active-kitty-window-id'
}

define-command kakoune-scrollback-generate-tmux-conf %{
    edit -scratch *tmux-conf*
    execute-keys '%d'
    evaluate-commands %sh{
        conf=$(kakoune-scrollback --generate-tmux-conf 2>&1)
        if [ $? -ne 0 ]; then
            err=$(printf '%s' "$conf" | tr '\r\n' '  ' | sed "s/'/''/g" | head -c 200)
            echo "fail 'failed to generate tmux conf: ${err}'"
        else
            printf "set-register '\"' '%s'\n" "$(printf '%s' "$conf" | sed "s/'/''/g")"
        fi
    }
    execute-keys '<a-P>gg'
    echo -markup '{Information}tmux.conf configuration written to *tmux-conf* buffer. Copy and paste into your tmux.conf.{Default}'
}
