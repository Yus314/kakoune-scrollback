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

# --- Summary ---

echo ""
echo "==========================="
printf "Results: \033[32m%d passed\033[0m, \033[31m%d failed\033[0m, %d total\n" "$passed" "$failed" "$total"

if [ "$failed" -gt 0 ]; then
    exit 1
fi
