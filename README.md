# Herdr Colors

Give each Herdr pane a distinct color.

Installation downloads themes and generates palettes. New panes apply them automatically; `herdr-colorize` repaints existing panes at a shell prompt.

## Install

[Herdr Setup](https://github.com/ariel-ps/herdr-setup) installs prerequisites and lets you select this plugin in `dependencies.json`.

For standalone installation, you need Herdr 0.9.3+, Git, Python 3, uv, zsh, and jq:

```sh
herdr plugin install ariel-ps/herdr-colors --ref main --yes
```

Use a commit or release tag instead of `main` to pin a version. Supports macOS, Ubuntu/Debian, and Fedora.

Herdr Setup loads the enabled plugin's helpers in bash or zsh. For a manual installation, source the installed plugin's `shell.bash` in `.bashrc` or `shell.zsh` in `.zshrc`. Bash helpers call the same zsh implementation, so zsh must also be installed; you keep bash as your shell.

## No colors?

Open a new terminal after installation, then launch Herdr. In a Herdr pane, run:

```sh
herdr-themes-build
herdr-colorize
```

This also repairs older installations that downloaded themes without generating palettes. Existing palettes are preserved during upgrades; `herdr-themes-build` explicitly rebuilds them. Repainting is visible at a shell prompt; a full-screen agent may draw its own background over the pane color.

## Repository layout

- `shell.zsh` and `shell.bash` are the public shell loaders.
- `scripts/build/sync-themes.sh` downloads the theme pool during installation.
- `libexec/theme-cache.py` privately builds the cached pane palettes.
- `vendor/theme-ga/theme-ga.py` selects distinct themes; its available provenance and licensing information is recorded in [`vendor/theme-ga/ORIGIN.md`](vendor/theme-ga/ORIGIN.md).

Downloaded themes and generated palettes stay in the user's XDG cache and are not committed to this repository.

See [CHANGELOG.md](CHANGELOG.md) for release history and [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

Original project code is licensed under the [MIT License](LICENSE). Third-party code and media retain their own terms; this license does not grant rights to game assets, downloaded themes, or other third-party content.
