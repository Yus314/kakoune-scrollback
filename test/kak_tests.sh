#!/usr/bin/env bash
# test/kak_tests.sh — Kakoune integration tests for kakoune-scrollback.kak
#
# Tests the plugin script in isolation using kak's headless dummy UI.
# Requires: kak and timeout (GNU coreutils) on PATH.
# Usage: ./test/kak_tests.sh

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PLUGIN="$PROJECT_DIR/rc/kakoune-scrollback.kak"
TIMEOUT_SEC=5

passed=0
failed=0
total=0

# --- Prerequisites ---

if ! command -v kak >/dev/null 2>&1; then
    echo "SKIP: kak not found on PATH"
    exit 0
fi

TIMEOUT_CMD=""
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD="gtimeout"
else
    echo "SKIP: timeout/gtimeout command not found"
    exit 0
fi

TMPDIR_TEST=$(mktemp -d)
trap 'rm -rf "$TMPDIR_TEST"' EXIT
RESULT="$TMPDIR_TEST/result"
KAK_STDERR="$TMPDIR_TEST/kak_stderr"

# Set XDG_RUNTIME_DIR for CI/container environments where it may be unset
export XDG_RUNTIME_DIR="$TMPDIR_TEST/kak-runtime"
mkdir -p "$XDG_RUNTIME_DIR"

# --- Helpers ---

run_kak() {
    "$TIMEOUT_CMD" "$TIMEOUT_SEC" kak -n -ui dummy -e "$1" </dev/null 2>"$KAK_STDERR"
}

run_kak_quiet() {
    "$TIMEOUT_CMD" "$TIMEOUT_SEC" kak -n -ui dummy -e "$1" </dev/null >/dev/null 2>"$KAK_STDERR"
}

assert_ok() {
    local name="$1"
    local commands="$2"
    total=$((total + 1))
    if run_kak_quiet "$commands"; then
        printf '  \033[32mPASS\033[0m: %s\n' "$name"
        passed=$((passed + 1))
    else
        printf '  \033[31mFAIL\033[0m: %s\n' "$name"
        if [ -s "$KAK_STDERR" ]; then
            printf '        stderr: %s\n' "$(cat "$KAK_STDERR")"
        fi
        failed=$((failed + 1))
    fi
}

assert_file_eq() {
    local name="$1"
    local file="$2"
    local expected="$3"
    total=$((total + 1))
    local actual
    actual=$(cat "$file" 2>/dev/null) || actual="<read error>"
    if [ "$actual" = "$expected" ]; then
        printf '  \033[32mPASS\033[0m: %s\n' "$name"
        passed=$((passed + 1))
    else
        printf '  \033[31mFAIL\033[0m: %s (expected "%s", got "%s")\n' "$name" "$expected" "$actual"
        if [ -s "$KAK_STDERR" ]; then
            printf '        stderr: %s\n' "$(cat "$KAK_STDERR")"
        fi
        failed=$((failed + 1))
    fi
}

# --- Tests ---

echo "=== Kakoune integration tests ==="
echo ""

# 1. Plugin loading
echo "Plugin loading:"

assert_ok "plugin sources without errors" \
    "source '$PLUGIN'; quit! 0"

# 2. Command definitions
echo ""
echo "Command definitions:"

assert_ok "kakoune-scrollback-help" \
    "source '$PLUGIN'; kakoune-scrollback-help; quit! 0"

assert_ok "kakoune-scrollback-generate-kitty-conf" \
    "source '$PLUGIN'; kakoune-scrollback-generate-kitty-conf; quit! 0"

assert_ok "kakoune-scrollback-setup-keymaps" \
    "source '$PLUGIN'; edit -scratch *test*; kakoune-scrollback-setup-keymaps; quit! 0"

assert_ok "kakoune-scrollback-setup-compose-keymaps" \
    "source '$PLUGIN'; edit -scratch *compose*; kakoune-scrollback-setup-compose-keymaps; quit! 0"

# quit command terminates kak; success = command exists and runs
assert_ok "kakoune-scrollback-quit" \
    "source '$PLUGIN'; kakoune-scrollback-quit"

# 3. Option declarations
echo ""
echo "Option declarations:"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        if [ -n \"\${kak_opt_scrollback_kitty_window_id+x}\" ]; then
            printf PASS > '$RESULT'
        else
            printf FAIL > '$RESULT'
        fi
    }
    quit! 0
" || true
assert_file_eq "scrollback_kitty_window_id is declared" "$RESULT" "PASS"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        if [ -n \"\${kak_opt_scrollback_tmp_dir+x}\" ]; then
            printf PASS > '$RESULT'
        else
            printf FAIL > '$RESULT'
        fi
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmp_dir is declared" "$RESULT" "PASS"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        if [ -n \"\${kak_opt_scrollback_compose_init+x}\" ]; then
            printf PASS > '$RESULT'
        else
            printf FAIL > '$RESULT'
        fi
    }
    quit! 0
" || true
assert_file_eq "scrollback_compose_init is declared" "$RESULT" "PASS"

# 4. Option defaults
echo ""
echo "Option defaults:"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_kitty_window_id\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_kitty_window_id defaults to empty" "$RESULT" ""

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_tmp_dir\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmp_dir defaults to empty" "$RESULT" ""

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_compose_init\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_compose_init defaults to empty" "$RESULT" ""

# 5. Keymaps
echo ""
echo "Keymaps:"

assert_ok "keymaps setup runs without error" \
    "source '$PLUGIN'; edit -scratch *test*; kakoune-scrollback-setup-keymaps; quit! 0"

# q keymap triggers kakoune-scrollback-quit (verify mapping via side effect,
# because quit! inside execute-keys doesn't terminate kak in dummy UI mode)
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    define-command -override kakoune-scrollback-quit %{
        nop %sh{ printf QUIT > '$RESULT' }
    }
    kakoune-scrollback-setup-keymaps
    execute-keys -with-maps q
    quit!
" || true
assert_file_eq "q keymap triggers quit command" "$RESULT" "QUIT"

# ? keymap shows help info without error
assert_ok "? keymap shows help" \
    "source '$PLUGIN'; edit -scratch *test*; kakoune-scrollback-setup-keymaps; execute-keys -with-maps ?
quit!"

# User extension point: kakoune-scrollback-user-keymaps (Approach E)
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    define-command kakoune-scrollback-user-keymaps %{
        define-command -override kakoune-scrollback-quit %{
            nop %sh{ printf CUSTOM > '$RESULT' }
        }
        map buffer normal Q ':kakoune-scrollback-quit<ret>'
    }
    kakoune-scrollback-setup-keymaps
    execute-keys -with-maps Q
    quit!
" || true
assert_file_eq "user-keymaps command overrides Q" "$RESULT" "CUSTOM"

# User extension point: trigger-user-hook scrollback-keymaps-ready (Approach F)
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    hook global User scrollback-keymaps-ready %{
        define-command -override kakoune-scrollback-quit %{
            nop %sh{ printf HOOK > '$RESULT' }
        }
        map buffer normal Q ':kakoune-scrollback-quit<ret>'
    }
    kakoune-scrollback-setup-keymaps
    execute-keys -with-maps Q
    quit!
" || true
assert_file_eq "User hook overrides Q" "$RESULT" "HOOK"

# 6. Option mutability (global scope)
echo ""
echo "Option mutability (global):"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    set-option global scrollback_kitty_window_id '42'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_kitty_window_id\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_kitty_window_id can be set (global)" "$RESULT" "42"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    set-option global scrollback_tmp_dir '/tmp/test-dir'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_tmp_dir\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmp_dir can be set (global)" "$RESULT" "/tmp/test-dir"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    set-option global scrollback_compose_init 'initial text'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_compose_init\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_compose_init can be set (global)" "$RESULT" "initial text"

# 7. Option mutability (buffer scope)
echo ""
echo "Option mutability (buffer):"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option buffer scrollback_tmp_dir '/tmp/buf-dir'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_tmp_dir\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmp_dir can be set (buffer)" "$RESULT" "/tmp/buf-dir"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option buffer scrollback_compose_init 'buf init text'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_compose_init\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_compose_init can be set (buffer)" "$RESULT" "buf init text"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option buffer scrollback_kitty_window_id '99'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_kitty_window_id\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_kitty_window_id can be set (buffer)" "$RESULT" "99"

# 8. New tmux-related options
echo ""
echo "tmux options:"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        if [ -n \"\${kak_opt_scrollback_backend+x}\" ]; then
            printf PASS > '$RESULT'
        else
            printf FAIL > '$RESULT'
        fi
    }
    quit! 0
" || true
assert_file_eq "scrollback_backend is declared" "$RESULT" "PASS"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_backend\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_backend defaults to empty" "$RESULT" ""

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        if [ -n \"\${kak_opt_scrollback_tmux_pane_id+x}\" ]; then
            printf PASS > '$RESULT'
        else
            printf FAIL > '$RESULT'
        fi
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmux_pane_id is declared" "$RESULT" "PASS"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    evaluate-commands %sh{
        printf '%s' \"\$kak_opt_scrollback_tmux_pane_id\" > '$RESULT'
    }
    quit! 0
" || true
assert_file_eq "scrollback_tmux_pane_id defaults to empty" "$RESULT" ""

# 9. Backend dispatch
echo ""
echo "Backend dispatch:"

# dispatch with backend=kitty resolves to kitty command
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option global scrollback_backend 'kitty'
    set-option global scrollback_kitty_window_id '1'
    define-command -override kakoune-scrollback-send-to-kitty %{
        nop %sh{ printf KITTY > '$RESULT' }
    }
    kakoune-scrollback-send
    quit!
" || true
assert_file_eq "dispatch send with backend=kitty" "$RESULT" "KITTY"

# dispatch with backend=tmux resolves to tmux command
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option global scrollback_backend 'tmux'
    set-option global scrollback_tmux_pane_id '%5'
    define-command -override kakoune-scrollback-send-to-tmux %{
        nop %sh{ printf TMUX > '$RESULT' }
    }
    kakoune-scrollback-send
    quit!
" || true
assert_file_eq "dispatch send with backend=tmux" "$RESULT" "TMUX"

# dispatch with empty backend fails
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    try %{
        kakoune-scrollback-send
    } catch %{
        nop %sh{ printf CAUGHT > '$RESULT' }
    }
    quit!
" || true
assert_file_eq "dispatch fails when backend not set" "$RESULT" "CAUGHT"

# dispatch with unknown backend fails
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option global scrollback_backend 'unknown'
    try %{
        kakoune-scrollback-send
    } catch %{
        nop %sh{ printf CAUGHT > '$RESULT' }
    }
    quit!
" || true
assert_file_eq "dispatch fails with unknown backend" "$RESULT" "CAUGHT"

# execute-target dispatch
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option global scrollback_backend 'kitty'
    define-command -override kakoune-scrollback-execute-in-kitty %{
        nop %sh{ printf EXEC_KITTY > '$RESULT' }
    }
    kakoune-scrollback-execute-target
    quit!
" || true
assert_file_eq "dispatch execute-target with backend=kitty" "$RESULT" "EXEC_KITTY"

# open-compose dispatch
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    set-option global scrollback_backend 'tmux'
    define-command -override kakoune-scrollback-compose-tmux %{
        nop %sh{ printf COMPOSE_TMUX > '$RESULT' }
    }
    kakoune-scrollback-open-compose
    quit!
" || true
assert_file_eq "dispatch open-compose with backend=tmux" "$RESULT" "COMPOSE_TMUX"

# 10. tmux command definitions
echo ""
echo "tmux command definitions:"

assert_ok "kakoune-scrollback-require-backend" \
    "source '$PLUGIN'; set-option global scrollback_backend 'kitty'; kakoune-scrollback-require-backend; quit! 0"

# 11. Compose submit/cancel
echo ""
echo "Compose submit/cancel:"

RESULT2="$TMPDIR_TEST/result2"

# submit: send is called with buffer contents
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *compose*
    execute-keys 'ihello world<esc>'
    set-option global scrollback_backend 'kitty'
    set-option global scrollback_kitty_window_id '1'
    define-command -override kakoune-scrollback-send %{
        nop %sh{ printf '%s' \"\$kak_selection\" > '$RESULT' }
    }
    kakoune-scrollback-submit
" || true
# $(cat) strips trailing newline; selection includes kak's trailing \n
assert_file_eq "submit sends buffer contents" "$RESULT" "hello world"

# submit-exec: both send and execute-target are called
> "$RESULT"
> "$RESULT2"
run_kak "
    source '$PLUGIN'
    edit -scratch *compose*
    execute-keys 'itest line<esc>'
    set-option global scrollback_backend 'kitty'
    set-option global scrollback_kitty_window_id '1'
    define-command -override kakoune-scrollback-send %{
        nop %sh{ printf '%s' \"\$kak_selection\" > '$RESULT' }
    }
    define-command -override kakoune-scrollback-execute-target %{
        nop %sh{ printf EXECUTED > '$RESULT2' }
    }
    kakoune-scrollback-submit-exec
" || true
assert_file_eq "submit-exec sends buffer contents" "$RESULT" "test line"
assert_file_eq "submit-exec calls execute-target" "$RESULT2" "EXECUTED"

# cancel: send is NOT called
> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *compose*
    execute-keys 'ishould not send<esc>'
    set-option global scrollback_backend 'kitty'
    set-option global scrollback_kitty_window_id '1'
    define-command -override kakoune-scrollback-send %{
        nop %sh{ printf SENT > '$RESULT' }
    }
    kakoune-scrollback-cancel
" || true
assert_file_eq "cancel does not call send" "$RESULT" ""

# 12. Send command stubs
echo ""
echo "Send command stubs:"

STUB_DIR="$TMPDIR_TEST/stub_bin"
mkdir -p "$STUB_DIR"
KITTY_LOG="$TMPDIR_TEST/kitty_log"
KITTY_STDIN="$TMPDIR_TEST/kitty_stdin"
TMUX_LOG="$TMPDIR_TEST/tmux_log"

# Create kitty stub
cat > "$STUB_DIR/kitty" << 'STUB'
#!/bin/sh
printf '%s\n' "$*" >> "$KITTY_LOG"
cat > "$KITTY_STDIN"
exit 0
STUB
chmod +x "$STUB_DIR/kitty"

# Create tmux stub
cat > "$STUB_DIR/tmux" << 'STUB'
#!/bin/sh
printf '%s\n' "$*" >> "$TMUX_LOG"
if [ "$1" = "load-buffer" ]; then
    cat > /dev/null
fi
exit 0
STUB
chmod +x "$STUB_DIR/tmux"

# Save PATH so we can restore after stub tests
SAVED_PATH="$PATH"

# kitty stub: stdin receives selection
> "$KITTY_LOG"
> "$KITTY_STDIN"
export PATH="$STUB_DIR:$PATH"
export KITTY_LOG KITTY_STDIN
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    execute-keys 'ihello kitty<esc>%'
    set-option global scrollback_backend 'kitty'
    set-option global scrollback_kitty_window_id '42'
    kakoune-scrollback-send-to-kitty
    quit!
" || true
# $(cat) strips trailing newline; selection includes kak's trailing \n
assert_file_eq "kitty stub receives selection on stdin" "$KITTY_STDIN" "hello kitty"

# kitty stub: --match=id:42 is in arguments
> "$RESULT"
if grep -q 'match=id:42' "$KITTY_LOG" 2>/dev/null; then
    printf FOUND > "$RESULT"
fi
assert_file_eq "kitty args contain --match=id:42" "$RESULT" "FOUND"

# tmux stub: load-buffer and paste-buffer are called
> "$TMUX_LOG"
export TMUX_LOG
run_kak "
    source '$PLUGIN'
    edit -scratch *test*
    execute-keys 'itmux text<esc>%'
    set-option global scrollback_backend 'tmux'
    set-option global scrollback_tmux_pane_id '%7'
    kakoune-scrollback-send-to-tmux
    quit!
" || true
> "$RESULT"
if grep -q 'load-buffer' "$TMUX_LOG" 2>/dev/null && grep -q 'paste-buffer' "$TMUX_LOG" 2>/dev/null; then
    printf BOTH > "$RESULT"
fi
assert_file_eq "tmux stub: load-buffer + paste-buffer called" "$RESULT" "BOTH"

# tmux stub: pane ID %7 is passed to paste-buffer
> "$RESULT"
if grep 'paste-buffer' "$TMUX_LOG" 2>/dev/null | grep -q '%7'; then
    printf FOUND > "$RESULT"
fi
assert_file_eq "tmux paste-buffer receives pane ID %7" "$RESULT" "FOUND"

# Restore PATH
export PATH="$SAVED_PATH"

# 13. Compose esc keymap
echo ""
echo "Compose keymaps:"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    edit -scratch *compose*
    execute-keys 'ishould cancel<esc>'
    define-command -override kakoune-scrollback-cancel %{
        nop %sh{ printf CANCELLED > '$RESULT' }
    }
    kakoune-scrollback-setup-compose-keymaps
    execute-keys -with-maps <esc>
    quit!
" || true
assert_file_eq "esc keymap triggers cancel" "$RESULT" "CANCELLED"

# 14. generate-tmux-conf
echo ""
echo "generate-tmux-conf:"

GEN_STUB_DIR="$TMPDIR_TEST/gen_stub_bin"
mkdir -p "$GEN_STUB_DIR"

# generate-tmux-conf failure test: stub exits 1
cat > "$GEN_STUB_DIR/kakoune-scrollback" << 'STUB'
#!/bin/sh
echo "mock error" >&2
exit 1
STUB
chmod +x "$GEN_STUB_DIR/kakoune-scrollback"

SAVED_PATH2="$PATH"
export PATH="$GEN_STUB_DIR:$PATH"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    try %{ kakoune-scrollback-generate-tmux-conf } catch %{
        nop %sh{ printf CAUGHT > '$RESULT' }
    }
    quit!
" || true
assert_file_eq "generate-tmux-conf catches failure" "$RESULT" "CAUGHT"

# generate-tmux-conf success test: stub outputs conf with single quote
cat > "$GEN_STUB_DIR/kakoune-scrollback" << 'STUB'
#!/bin/sh
printf "bind-key -T copy-mode 'h' send-keys -X scrollback\n"
STUB
chmod +x "$GEN_STUB_DIR/kakoune-scrollback"

> "$RESULT"
run_kak "
    source '$PLUGIN'
    kakoune-scrollback-generate-tmux-conf
    evaluate-commands -buffer *tmux-conf* %{
        execute-keys '%'
        nop %sh{ printf '%s' \"\$kak_selection\" > '$RESULT' }
    }
    quit!
" || true
# Check that the tmux-conf buffer contains the single quote (escaped properly)
> "$RESULT2"
if grep -q "bind-key" "$RESULT" 2>/dev/null; then
    printf FOUND > "$RESULT2"
fi
assert_file_eq "generate-tmux-conf populates buffer" "$RESULT2" "FOUND"

export PATH="$SAVED_PATH2"

# --- Summary ---

echo ""
echo "==========================="
printf "Results: \033[32m%d passed\033[0m, \033[31m%d failed\033[0m, %d total\n" "$passed" "$failed" "$total"

if [ "$failed" -gt 0 ]; then
    exit 1
fi
