# Herdr Colors

Give each Herdr pane a distinct color.

Run `herdr-themes-build` once to generate palettes. New panes apply them automatically; `herdr-colorize` repaints panes at a shell prompt.

## Install

[Herdr Setup](https://github.com/ariel-ps/herdr-setup) installs prerequisites and lets you select this plugin in `dependencies.json`.

With Herdr 0.9.3+ already installed:

```sh
herdr plugin install ariel-ps/herdr-colors --ref main --yes
```

Use a commit or release tag instead of `main` to pin a version. Supports macOS and Ubuntu/Debian Linux.

Herdr Setup loads `shell.zsh` for enabled plugins when a new zsh starts. For a manual installation, source the installed plugin’s `shell.zsh` in your `.zshrc`.
