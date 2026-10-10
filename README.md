# Herdr Colors

**Give Herdr panes distinct, readable color slots.**

A Rust application selects dark palettes, checks text contrast, and requires color differences under simulated deuteranopia. New panes apply their cached colors automatically; existing panes can be repainted at a shell prompt. Colors repeat when the number of panes exceeds the configured slot count (16 by default).

```sh
herdr-colors build 16 --print
herdr-colors colorize
```

## Install

[Herdr Setup](https://github.com/ariel-ps/herdr-setup) installs Herdr and the build dependencies. Supports macOS, Ubuntu/Debian, and Fedora, with Bash or Zsh.

For standalone installation, you need Herdr 0.9.3+ and Rust 1.87 or newer (Cargo and rustc). Git is required unless `HERDR_KIT_THEMES` points to a local theme directory:

```sh
herdr plugin install ariel-ps/herdr-colors --ref main --yes
herdr plugin action invoke colorize --plugin dev.ariel.herdr-colors
```

Installation compiles the binary, downloads Kitty themes, and generates missing palettes. It preserves existing palettes and their cache location. The installed application needs Herdr and `ps` for repainting, and Git for theme downloads. Python, uv, jq, and Zsh are no longer runtime dependencies of this plugin.

Herdr Setup loads `shell.bash` or `shell.zsh` automatically. For a standalone installation, add the matching loader to your shell configuration, then open a new terminal:

```sh
# ~/.bashrc
source "${XDG_CONFIG_HOME:-$HOME/.config}"/herdr/plugins/github/dev.ariel.herdr-colors-*/shell.bash

# ~/.zshrc
source "${XDG_CONFIG_HOME:-$HOME/.config}"/herdr/plugins/github/dev.ariel.herdr-colors-*/shell.zsh
```

The loader adds the commands to `PATH` and applies cached colors inside Herdr panes when stdout is a terminal.

## Commands

| Command | Purpose |
| --- | --- |
| `herdr-colors build [COUNT]` | Select and cache palettes; defaults to 16 |
| `herdr-colors build --print` | Rebuild and print the selected theme names |
| `herdr-colors build --max-lightness 55` | Set the maximum background lightness |
| `herdr-colors colorize [PANE ...]` | Repaint selected panes, or every pane |
| `herdr-colors apply [PANE]` | Emit raw OSC color sequences; defaults to `HERDR_PANE_ID` |
| `herdr-colors detect` | Return current terminal location and neighbors as JSON |
| `herdr-colors options [PANE]` | Score every cached palette against the pane layout as JSON |
| `herdr-colors sync` | Download/update themes and repair missing or invalid palettes |

`herdr-themes-build` and `herdr-colorize` remain available with the same arguments. `apply --quiet` is available when a script must suppress diagnostics.

Colors are cached under `${XDG_CACHE_HOME:-$HOME/.cache}/herdr-pane-themes`; a relative `XDG_CACHE_HOME` is ignored. Existing Python-generated caches remain readable. Rebuilding is randomized and may choose a different valid set each time.

Set `HERDR_KIT_THEMES` to use a local directory of Kitty `.conf` themes. Each direct child `.conf` file must define `background`, `foreground`, and `color0` through `color15` as `#RRGGBB`. Selection requires CIE Lab L* 5–55 by default, background chroma 3–25, foreground contrast of at least 4.5, moderate palette colorfulness, and enough mutually distinguishable themes for `COUNT`. Malformed colors and duplicate definitions are rejected.

`sync` exits unsuccessfully if downloading or updating themes fails. It preserves a valid existing palette, so run `build` after `sync` when you want to select from newly downloaded themes.

## Detection API

`herdr-colors detect` always writes JSON. Inside Herdr it returns the current pane metadata, terminal ID, layout rectangle, total layout area, and the nearest pane in each cardinal direction:

```json
{
  "in_herdr": true,
  "has_pane": true,
  "pane": {"pane_id": "w1:p2", "terminal_id": "term_123"},
  "location": {"x": 0, "y": 24, "width": 80, "height": 24},
  "area": {"x": 0, "y": 0, "width": 120, "height": 48},
  "neighbors": {
    "left": null,
    "right": {"pane_id": "w1:p3", "location": {"x": 80, "y": 0, "width": 40, "height": 48}},
    "up": {"pane_id": "w1:p1", "location": {"x": 0, "y": 0, "width": 80, "height": 24}},
    "down": null
  }
}
```

Outside Herdr, `in_herdr` and `has_pane` are false and location fields are null. A Herdr query failure still returns a JSON error object and exits nonzero.

## Location-aware selection

`herdr-colors options [PANE]` returns every cached genetic palette with its normal and simulated-deuteranopia distance from touching panes. It also returns the deterministic genetic assignment for every pane in the tab:

```json
{
  "pane_id": "w1:p2",
  "strategy": "genetic-layout",
  "selected_slot": 12,
  "selected_background": "#011627",
  "touching_neighbors": [
    {"pane_id": "w1:p1", "slot": 8, "background": "#0b230a"}
  ],
  "assignment": [
    {"pane_id": "w1:p1", "slot": 8, "background": "#0b230a"},
    {"pane_id": "w1:p2", "slot": 12, "background": "#011627"}
  ],
  "options": [
    {
      "slot": 1,
      "background": "#232136",
      "minimum_distance": 21.35,
      "minimum_deuteranopia_distance": 17.93,
      "score": 21.35,
      "selected": false
    }
  ]
}
```

Inside Herdr, `apply` and `colorize` use this assignment automatically. Pane rectangles determine adjacency, and a stable layout-derived seed prevents colors from changing between identical runs. A pane without touching neighbors keeps its pane-ID slot and reports null distance/score fields. If layout detection fails, color application falls back to the pane-ID slot.

## No colors?

Inside Herdr, at a shell prompt:

```sh
herdr-themes-build
herdr-colorize
```

A full-screen agent may draw its own background over the pane color. If commands are missing, open a new terminal after installation. If themes are missing, run `herdr-colors sync`. To diagnose startup failures, run:

```sh
herdr-colors apply "$HERDR_PANE_ID"
```

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 tests/test_plugin.py
```

Python is used only by the integration test. It builds an isolated copy in a path containing spaces and checks Bash/Zsh loading, old caches, invalid inputs, and real pseudo-terminal writes with mocked Herdr responses.

Build in place with `sh scripts/build/install.sh`, then use `./bin/herdr-colors` or source the loader from this checkout. This downloads themes unless `HERDR_KIT_THEMES` points to a local collection. Cargo dependencies are recorded in `Cargo.lock`.

## Repository layout

- `src/` contains the Rust CLI, palette selection, and unit tests.
- `bin/` contains the installed binary and compatibility launchers.
- `shell.bash` and `shell.zsh` are thin shell loaders.
- `scripts/build/install.sh` compiles the executable and prepares palettes.
- `docs/selection-provenance.md` records the previous selector's origin.

See [CHANGELOG.md](CHANGELOG.md) for release history and [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

Original project code is licensed under the [MIT License](LICENSE). Third-party code and themes retain their own terms. See [selection provenance](docs/selection-provenance.md).
