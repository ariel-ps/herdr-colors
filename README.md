# Herdr Colors

**Give each Herdr pane a distinct, readable color.**

A Rust application selects dark palettes, checks text contrast, and considers color differences under simulated deuteranopia. New panes apply their cached colors automatically; existing panes can be repainted at a shell prompt.

```sh
herdr-colors build 16 --print
herdr-colors colorize
```

## Install

[Herdr Setup](https://github.com/ariel-ps/herdr-setup) installs Herdr and the build dependencies. Supports macOS, Ubuntu/Debian, and Fedora, with Bash or Zsh.

For standalone installation, you need Herdr 0.9.3+, Git, and a stable Rust toolchain (Cargo and rustc):

```sh
herdr plugin install ariel-ps/herdr-colors --ref main --yes
herdr plugin action invoke colorize --plugin dev.ariel.herdr-colors
```

Installation compiles the binary, downloads Kitty themes, and generates missing palettes. It preserves existing palettes and their cache location. The installed application needs Herdr and `ps` for repainting, and Git for theme downloads. Python, uv, jq, and Zsh are no longer runtime dependencies of this plugin.

Herdr Setup loads `shell.bash` or `shell.zsh` automatically. For a standalone installation, source the installed plugin's loader from `.bashrc` or `.zshrc`, then open a new terminal. The loader adds the commands to `PATH` and applies cached colors inside Herdr panes.

## Commands

| Command | Purpose |
| --- | --- |
| `herdr-colors build [COUNT]` | Select and cache palettes; defaults to 16 |
| `herdr-colors build --print` | Rebuild and print the selected theme names |
| `herdr-colors build --max-lightness 55` | Set the maximum background lightness |
| `herdr-colors colorize [PANE ...]` | Repaint selected panes, or every pane |
| `herdr-colors apply [PANE]` | Emit colors for a pane; defaults to `HERDR_PANE_ID` |
| `herdr-colors sync` | Download/update themes and prepare missing palettes |

`herdr-themes-build` and `herdr-colorize` remain available with the same arguments. `apply --quiet` suppresses diagnostics during shell startup.

Colors are cached under `${XDG_CACHE_HOME:-$HOME/.cache}/herdr-pane-themes`. Existing Python-generated caches remain readable. Set `HERDR_KIT_THEMES` to use a local directory of Kitty `.conf` themes. Malformed colors and duplicate color definitions are rejected before selection.

## No colors?

Inside Herdr, at a shell prompt:

```sh
herdr-themes-build
herdr-colorize
```

A full-screen agent may draw its own background over the pane color. If commands are missing, open a new terminal after installation. If themes are missing, run `herdr-colors sync`.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 tests/test_plugin.py
```

Python is used only by the integration test. It builds an isolated copy in a path containing spaces and checks Bash/Zsh loading, old caches, invalid inputs, and real pseudo-terminal writes with mocked Herdr responses.

Build and install locally with `sh scripts/build/install.sh`. This downloads themes unless `HERDR_KIT_THEMES` points to a local collection. Cargo dependencies are recorded in `Cargo.lock`.

## Repository layout

- `src/` contains the Rust CLI, palette selection, and unit tests.
- `bin/` contains the installed binary and compatibility launchers.
- `shell.bash` and `shell.zsh` are thin shell loaders.
- `scripts/build/install.sh` compiles the executable and prepares palettes.
- `docs/selection-provenance.md` records the previous selector's origin.

See [CHANGELOG.md](CHANGELOG.md) for release history and [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

Original project code is licensed under the [MIT License](LICENSE). Third-party code and themes retain their own terms. See [selection provenance](docs/selection-provenance.md).
