# Source from .bashrc; all runtime work is handled by the Rust binary.
if [[ -n ${HERDR_PLUGIN_ROOT:-} ]]; then
  _HERDR_COLORS_ROOT=$(CDPATH='' cd -- "$HERDR_PLUGIN_ROOT" && pwd -P)
else
  _HERDR_COLORS_ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
fi
export PATH="$_HERDR_COLORS_ROOT/bin:$PATH"
if [[ $- == *i* && -n ${HERDR_PANE_ID:-} && -x $_HERDR_COLORS_ROOT/bin/herdr-colors ]]; then
  "$_HERDR_COLORS_ROOT/bin/herdr-colors" apply --quiet || :
fi
