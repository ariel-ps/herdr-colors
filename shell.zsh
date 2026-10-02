# Source this file from zsh to load this plugin's commands.
typeset -g _HERDR_COLORS_ROOT="${HERDR_PLUGIN_ROOT:-${0:A:h}}"
_HERDR_COLORS_ROOT="${_HERDR_COLORS_ROOT:A}"
typeset -U path
path=("$_HERDR_COLORS_ROOT/bin" $path)

__herdr_colors_ready() {
  command -v herdr >/dev/null 2>&1 || { echo "herdr: not installed" >&2; return 1; }
  command -v jq >/dev/null 2>&1 || { echo "herdr: jq not found" >&2; return 1; }
  [ "$(herdr status server 2>/dev/null | awk '/^status:/{print $2}')" = running ] || {
    echo "herdr: server not running" >&2; return 1
  }
}

__herdr_theme_cache() { print -r -- "${XDG_CACHE_HOME:-$HOME/.cache}/herdr-pane-themes"; }

# usage: herdr-themes-build [count] [--print]
#   Slow and deliberate; the result is what every new pane reads at startup.
herdr-themes-build() {
  uv run --no-project python "$_HERDR_COLORS_ROOT/libexec/theme-cache.py" "$@"
}

# Resolve a pane id to its cached OSC payload. Silent on anything unexpected so
# a missing cache is never a broken prompt.
__herdr_payload_for() {
  local dir count slot
  dir=$(__herdr_theme_cache)
  [ -r "$dir/current" ] || return 1
  count=$(<"$dir/current")
  [[ "$count" == <1-> ]] || return 1
  [ -r "$dir/$count.txt" ] || return 1
  # Pane ids run p1..p9 then pA, pB — base 36, not decimal. Checked by stripping
  # every alnum rather than a glob, since the sourcing emulate has no extendedglob.
  slot=${(U)1##*p}
  [ -n "$slot" ] && [ -z "${slot//[0-9A-Z]/}" ] || return 1
  slot=$(( 36#$slot ))
  sed -n "$(( (slot - 1) % count + 1 ))p" "$dir/$count.txt"
}

__herdr_apply_pane_theme() {
  [[ -o interactive ]] || return 0
  [ -n "${HERDR_PANE_ID:-}" ] || return 0
  local payload
  payload=$(__herdr_payload_for "$HERDR_PANE_ID") || return 0
  [ -n "$payload" ] && printf '%b' "$payload"
  return 0
}

# usage: herdr-colorize [pane-id ...]
#   Repaint existing panes without restarting them, by writing the sequence to
#   each pane's pty slave: those bytes surface on the master side as ordinary
#   pane output, which is the emulator's input, never the agent's.
#
#   Only panes at a shell prompt change visibly. A pane running a full-screen
#   agent accepts the write and looks identical, because that agent is painting
#   its own background over the viewport — the reported success is the pty
#   accepting bytes, not the screen changing. Use it after a rebuild or once an
#   agent exits; for a running one the alerts plugin composites a layer instead.
herdr-colorize() {
  __herdr_colors_ready || return 1
  local -a panes
  if (( $# )); then
    panes=("$@")
  else
    panes=("${(@f)$(herdr pane list | jq -r '.result.panes[].pane_id')}")
  fi
  local pane pid tty payload rc=0
  for pane in $panes; do
    payload=$(__herdr_payload_for "$pane") || {
      echo "$pane: no cached palette. Run herdr-themes-build, then herdr-colorize." >&2
      rc=1; continue
    }
    pid=$(herdr pane process-info --pane "$pane" 2>/dev/null | jq -r '.result.process_info.shell_pid // empty')
    [ -n "$pid" ] || { echo "$pane: no shell pid" >&2; rc=1; continue; }
    tty=$(ps -o tty= -p "$pid" 2>/dev/null | tr -d ' ')
    [ -n "$tty" ] && [ -w "/dev/$tty" ] || { echo "$pane: no writable tty" >&2; rc=1; continue; }
    printf '%b' "$payload" > "/dev/$tty" && echo "$pane -> /dev/$tty" || rc=1
  done
  return $rc
}


__herdr_apply_pane_theme
