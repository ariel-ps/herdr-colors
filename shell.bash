# Bash entry points reuse the plugin's zsh implementation.
_HERDR_COLORS_ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
export PATH="$_HERDR_COLORS_ROOT/bin:$PATH"

herdr-themes-build() {
  zsh -fc 'source "$1/shell.zsh"; shift; herdr-themes-build "$@"' herdr-themes-build "$_HERDR_COLORS_ROOT" "$@"
}

herdr-colorize() {
  zsh -fc 'source "$1/shell.zsh"; shift; herdr-colorize "$@"' herdr-colorize "$_HERDR_COLORS_ROOT" "$@"
}

# Apply the cached color to this bash pane without loading zsh startup files.
if [[ $- == *i* && -n ${HERDR_PANE_ID:-} ]]; then
  zsh -fc 'source "$1/shell.zsh"; payload=$(__herdr_payload_for "$HERDR_PANE_ID") && printf "%b" "$payload"; exit 0' herdr-colors "$_HERDR_COLORS_ROOT"
fi
