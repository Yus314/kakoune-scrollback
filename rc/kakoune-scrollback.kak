# kakoune-scrollback — Kitty scrollback viewer for Kakoune

# Global options (accessible from compose client)
declare-option -hidden str scrollback_kitty_window_id ''

# Buffer-local options
declare-option -hidden str scrollback_tmp_dir ''

# Paste window initial value
declare-option -hidden str scrollback_compose_init ''

# User-configurable options
declare-option str scrollback_max_lines '5000'

# --- Keymaps ---

define-command -hidden kakoune-scrollback-setup-keymaps %{
    map buffer normal q     ':kakoune-scrollback-quit<ret>'
    map buffer normal y     ':kakoune-scrollback-yank<ret>'
    map buffer normal <ret> ':kakoune-scrollback-paste<ret>'
    map buffer normal !     ':kakoune-scrollback-execute<ret>'
    map buffer normal e     ':kakoune-scrollback-edit<ret>'
    map buffer normal ?     ':kakoune-scrollback-help<ret>'
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
    evaluate-commands %sh{
        printf '%s' "$kak_selection" | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --bracketed-paste \
            --stdin
    }
    kakoune-scrollback-quit
}

define-command kakoune-scrollback-execute %{
    evaluate-commands %sh{
        printf '%s' "$kak_selection" | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --bracketed-paste \
            --stdin
        printf '\r' | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --stdin
    }
    kakoune-scrollback-quit
}

define-command kakoune-scrollback-help %{
    info -title 'kakoune-scrollback' \
        'q      : quit
y      : yank selection to clipboard (OSC 52)
<ret>  : paste selection to Kitty
!      : execute selection in Kitty
e      : open paste window
         <a-s>   : submit (paste)
         <a-ret> : submit and execute
         <esc>   : cancel
?      : show this help'
}

# --- Paste window ---

define-command kakoune-scrollback-edit %{
    set-option global scrollback_compose_init %val{selection}

    edit -scratch *compose*

    try %{
        set-register '"' %opt{scrollback_compose_init}
        execute-keys '<a-P>'
    }

    evaluate-commands %sh{
        kitty @ launch --no-response --type=overlay \
            --env KAKOUNE_SCROLLBACK=1 \
            -- kak -c "$kak_session" -e '
                buffer *compose*
                map buffer normal <a-s>   ":kakoune-scrollback-submit<ret>"
                map buffer normal <a-ret> ":kakoune-scrollback-submit-exec<ret>"
                map buffer normal <esc>   ":kakoune-scrollback-cancel<ret>"
                execute-keys gi
            '
    }
}

define-command -hidden kakoune-scrollback-submit %{
    execute-keys '%y'
    evaluate-commands %sh{
        printf '%s' "$kak_reg_dquote" | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --bracketed-paste \
            --stdin
    }
    delete-buffer *compose*
    quit
}

define-command -hidden kakoune-scrollback-submit-exec %{
    execute-keys '%y'
    evaluate-commands %sh{
        printf '%s' "$kak_reg_dquote" | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --bracketed-paste \
            --stdin
        printf '\r' | kitty @ send-text \
            --match="id:${kak_opt_scrollback_kitty_window_id}" \
            --stdin
    }
    delete-buffer *compose*
    quit
}

define-command -hidden kakoune-scrollback-cancel %{
    delete-buffer *compose*
    quit
}

# --- Kitty configuration helper ---

define-command kakoune-scrollback-generate-kitty-conf %{
    echo -markup '{Information}Add the following to your kitty.conf:{Default}

# kakoune-scrollback
allow_remote_control yes
listen_on unix:/tmp/kitty

map ctrl+shift+h launch --type=overlay \
    --env KAKOUNE_SCROLLBACK_TARGET_WINDOW_ID=$KITTY_WINDOW_ID \
    --stdin-source=@screen_scrollback \
    --stdin-add-formatting \
    --stdin-add-line-wrap-markers \
    kakoune-scrollback

map ctrl+shift+g launch --type=overlay \
    --env KAKOUNE_SCROLLBACK_TARGET_WINDOW_ID=$KITTY_WINDOW_ID \
    --stdin-source=@last_cmd_output \
    --stdin-add-formatting \
    kakoune-scrollback'
}
