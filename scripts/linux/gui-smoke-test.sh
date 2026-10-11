#!/usr/bin/env bash
# Headless GUI smoke test for GitComet on Linux.
#
# Boots the app under Xvfb + matchbox-window-manager with software Vulkan
# (lavapipe), drives it with xdotool against a throwaway fixture repo, and
# asserts on FUNCTIONAL SIDE EFFECTS, not screenshots.
#
# Why not screenshots: GPUI/lavapipe presents through a direct scanout path
# that Xvfb's root buffer never sees. The first frame captured is real, but
# every later frame is byte-identical (verified: resize, keystrokes, and
# XSendEvent all produce AE=0 diffs while the app demonstrably processes
# input). Pixel assertions in this environment are structurally impossible;
# the app's own behavior is the observable instead.
#
# The core check: ctrl+grave opens the terminal panel, then a typed shell
# command lands in a real PTY (marker file proves keystroke -> PTY -> disk).
#
# Designed to run INSIDE the gitcomet-gui Docker image (ships Xvfb, matchbox,
# xdotool, vim, htop, mesa):
#
#   docker run --rm --shm-size=1g \
#     -v "$HOME:$HOME" -w "$HOME/Projects/gitcomet" \
#     -e HOME="$HOME" gitcomet-gui bash scripts/linux/gui-smoke-test.sh
#
# Overrides: GITCOMET_BIN (binary path), OUT (evidence dir), REPO (fixture
# repo dir), DISPLAY_NUM (Xvfb display number).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${GITCOMET_BIN:-$ROOT/target-gui/debug/gitcomet}"
OUT="${OUT:-$HOME/.cache/gitcomet-smoke}"
REPO="${REPO:-/tmp/gitcomet-smoke-repo}"
DISPLAY_NUM="${DISPLAY_NUM:-99}"
MARKER_DIR="$HOME/.cache/gitcomet-smoke-markers.$$"

if [[ ! -x "$BIN" ]]; then
  echo "FAIL: no binary at $BIN (build it or set GITCOMET_BIN)" >&2
  exit 1
fi
for tool in Xvfb matchbox-window-manager xdotool; do
  command -v "$tool" >/dev/null || { echo "FAIL: missing $tool (run inside gitcomet-gui image)" >&2; exit 1; }
done

mkdir -p "$OUT"; rm -f "$OUT"/*.png "$OUT"/wmctrl.txt
rm -rf "$MARKER_DIR"; mkdir -p "$MARKER_DIR"

# --- fixture repo: one commit, one dirty file, session pre-loads it ---
rm -rf "$REPO"; mkdir -p "$REPO"; cd "$REPO"
git init -q; git config user.email s@s; git config user.name s
seq 1 50 | sed 's/^/line /' > a.txt; git add -A; git commit -qm init
echo dirty > b.txt
SESSION="$(mktemp /tmp/gitcomet-smoke-session.XXXXXX.json)"
echo "{\"version\":2,\"open_repos\":[\"$REPO\"],\"active_repo\":\"$REPO\"}" > "$SESSION"

# --- display + WM ---
export DISPLAY=":$DISPLAY_NUM"
Xvfb "$DISPLAY" -screen 0 1600x1000x24 >/tmp/gitcomet-smoke-xvfb.log 2>&1 &
XVFB=$!
sleep 1
matchbox-window-manager >/tmp/gitcomet-smoke-wm.log 2>&1 &
WM=$!
mkdir -p /tmp/xdg && export XDG_RUNTIME_DIR=/tmp/xdg
sleep 1

APP=""
cleanup() {
  [[ -n "$APP" ]] && kill "$APP" 2>/dev/null
  kill "$WM" "$XVFB" 2>/dev/null
  pkill -f "$(basename "$BIN")" 2>/dev/null
  rm -f "$SESSION"
  true
}
trap cleanup EXIT

# --- app ---
export LIBGL_ALWAYS_SOFTWARE=1
export GITCOMET_SESSION_FILE="$SESSION"
export GIT_CONFIG_NOSYSTEM=1
"$BIN" >"$OUT/app.log" 2>&1 &
APP=$!

shot() { import -window root "$OUT/$1.png" 2>/dev/null || true; }   # best-effort evidence only
key()  { xdotool key --clearmodifiers "$1"; sleep "${2:-1}"; }
type() { xdotool type --delay 30 "$1"; }

FAILURES=0
fail() { echo "FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
wait_for() {  # wait_for <seconds> <command...>
  local deadline=$((SECONDS + $1)); shift
  until "$@"; do
    [[ $SECONDS -ge $deadline ]] && return 1
    sleep 0.5
  done
  return 0
}

# --- boot ---
for i in $(seq 1 90); do
  xdotool search --name "GitComet" >/dev/null 2>&1 && break
  kill -0 "$APP" 2>/dev/null || { echo "FAIL: app died during startup" >&2; tail -30 "$OUT/app.log" >&2; exit 1; }
  sleep 1
done
xdotool search --name "GitComet" >/dev/null 2>&1 || { echo "FAIL: no GitComet window after 90s" >&2; exit 1; }
WID=$(xdotool search --name "GitComet" | head -1)
sleep 8
shot 01_main_window

# --- core check: ctrl+grave opens the terminal panel, keystrokes reach a PTY ---
key "ctrl+grave" 3
type "touch $MARKER_DIR/pty-open"
key Return 4
wait_for 5 test -f "$MARKER_DIR/pty-open" || fail "ctrl+grave + typed command never reached a shell (terminal panel did not open or PTY dead)"
shot 02_terminal_open

# --- PTY is a real interactive shell: state persists across commands ---
type "cd $REPO && pwd > $MARKER_DIR/cwd"
key Return 4
wait_for 5 test -f "$MARKER_DIR/cwd" || fail "second PTY command did not execute"
grep -q "$REPO" "$MARKER_DIR/cwd" 2>/dev/null || fail "PTY cwd wrong: $(cat "$MARKER_DIR/cwd" 2>/dev/null)"

# --- throughput: 2000-line burst completes (exercises the render coalescer path) ---
type "seq 1 2000; echo burst-done > $MARKER_DIR/burst"
key Return 6
wait_for 10 test -f "$MARKER_DIR/burst" || fail "2000-line burst never completed"
shot 03_after_burst

# --- window stays alive and mapped through the whole run ---
kill -0 "$APP" 2>/dev/null || fail "app died during the run"
wmctrl -l > "$OUT/wmctrl.txt" 2>/dev/null
grep -q "GitComet" "$OUT/wmctrl.txt" || fail "window missing from wmctrl"

if [[ $FAILURES -eq 0 ]]; then
  echo "PASS: GitComet GUI smoke test (evidence in $OUT, markers in $MARKER_DIR)"
else
  echo "FAIL: $FAILURES assertion(s) failed; see $OUT/app.log" >&2
fi
exit $((FAILURES > 0))