#!/bin/sh
set -eu

if [ -n "${HERDR_PLUGIN_ROOT:-}" ]; then
    root=$HERDR_PLUGIN_ROOT
else
    root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
fi
root=$(CDPATH= cd -- "$root" && pwd -P)

themes="${XDG_CACHE_HOME:-$HOME/.cache}/kitty-themes"
if [ -d "$themes/.git" ]; then
    git -C "$themes" pull --ff-only || echo 'themes: kept existing clone'
else
    mkdir -p "$(dirname "$themes")"
    git clone --depth 1 https://github.com/kovidgoyal/kitty-themes.git "$themes"
fi

# Shell startup only reads palettes; prepare them during installation.
cache="${XDG_CACHE_HOME:-$HOME/.cache}/herdr-pane-themes"
if [ ! -s "$cache/current" ]; then
    python3 "$root/libexec/theme-cache.py"
fi
