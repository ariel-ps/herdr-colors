# Source from .zshrc; all runtime work is handled by the Rust binary.
typeset -g _HERDR_COLORS_ROOT="${HERDR_PLUGIN_ROOT:-${0:A:h}}"
_HERDR_COLORS_ROOT="${_HERDR_COLORS_ROOT:A}"
typeset -U path
path=("$_HERDR_COLORS_ROOT/bin" $path)
if [[ -o interactive && -t 1 && -n ${HERDR_PANE_ID:-} && -x $_HERDR_COLORS_ROOT/bin/herdr-colors ]]; then
  "$_HERDR_COLORS_ROOT/bin/herdr-colors" apply || :
fi
