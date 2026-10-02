#!/bin/sh
set -eu
themes="${XDG_CACHE_HOME:-$HOME/.cache}/kitty-themes"
if [ -d "$themes/.git" ]; then
    git -C "$themes" pull --ff-only || echo 'themes: kept existing clone'
else
    mkdir -p "$(dirname "$themes")"
    git clone --depth 1 https://github.com/kovidgoyal/kitty-themes.git "$themes" || echo 'themes: download unavailable; pane colors use the Herdr theme'
fi
