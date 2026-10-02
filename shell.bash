# Source from .bashrc; all runtime work is handled by the Rust binary.
_HERDR_COLORS_ROOT=${HERDR_PLUGIN_ROOT:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)}
export PATH="$_HERDR_COLORS_ROOT/bin:$PATH"
if [[ $- == *i* && -n ${HERDR_PANE_ID:-} && -x $_HERDR_COLORS_ROOT/bin/herdr-colors ]]; then
  "$_HERDR_COLORS_ROOT/bin/herdr-colors" apply --quiet || :
fi
