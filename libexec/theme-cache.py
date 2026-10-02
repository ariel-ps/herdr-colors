#!/usr/bin/env python3
"""Precompute per-pane OSC color payloads for herdr panes.

herdr has one client-wide theme and no per-pane setting, but it leaves a pane's
default colors alone once a shell builtin sets them: the restore path only fires
when the bare shell owns the foreground and the pane is off the alternate screen
(src/pane/osc.rs, should_restore_host_terminal_theme). Agents live on the
alternate screen, so a colour emitted at shell startup survives the agent that
replaces the shell.

Picking themes is the expensive part, so it does not belong in a shell rc. This
runs the same GA that backs kitty-colorize-panes once, resolves each chosen
theme to a finished OSC string, and writes one payload per line. Sourced herdr
helpers then just read line N for their pane and print it.
"""
import argparse
import glob
import os
import re
import subprocess
import sys
import tempfile

# Resolved from this file, not from an install prefix: herdr hands a plugin its
# own root and nothing else, so the only reliable anchor is where the script is.
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GA = os.path.join(ROOT, "vendor", "theme-ga", "theme-ga.py")

CACHE = os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache"))
# The palette pool is a clone, not a committed asset —
# scripts/build/sync-themes.sh puts it here. A plugin that vendored 409 themes
# would be mostly other people's colour schemes.
THEMES = os.environ.get("HERDR_KIT_THEMES", os.path.join(CACHE, "kitty-themes/themes"))
CACHE_DIR = os.path.join(CACHE, "herdr-pane-themes")
PALETTE_KEYS = {f"color{i}": i for i in range(16)}


def parse_theme(path):
    """Read only literal RGB colors; theme text must never become OSC commands."""
    out = {"palette": {}}
    seen = set()
    with open(path, errors="replace") as fh:
        for line in fh:
            parts = line.split()
            if not parts or parts[0] not in {"background", "foreground", *PALETTE_KEYS}:
                continue
            if len(parts) != 2 or not re.fullmatch(r'#[0-9A-Fa-f]{6}', parts[1]) or parts[0] in seen:
                raise ValueError(f'{path}: invalid or duplicate color: {parts[0]}')
            key, value = parts
            seen.add(key)
            if key in ("background", "foreground"):
                out[key] = value
            elif key in PALETTE_KEYS:
                out["palette"][PALETTE_KEYS[key]] = value
    return out if "background" in out else None


def osc_payload(theme):
    """One printf-ready string: OSC 10/11 for defaults, OSC 4 for the palette."""
    seqs = [f"\\e]11;{theme['background']}\\e\\\\"]
    if "foreground" in theme:
        seqs.append(f"\\e]10;{theme['foreground']}\\e\\\\")
    for idx in sorted(theme["palette"]):
        seqs.append(f"\\e]4;{idx};{theme['palette'][idx]}\\e\\\\")
    return "".join(seqs)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("count", nargs="?", type=int, default=16,
                    help="how many distinct slots to precompute (default: 16)")
    ap.add_argument("--max-lightness", default="55.0",
                    help="dark-only pool; passed straight to the GA")
    ap.add_argument("--print", action="store_true", help="dump chosen theme names")
    args = ap.parse_args()
    if args.count < 1:
        ap.error('count must be at least 1')

    for path, what in ((GA, "theme GA"), (THEMES, "theme cache")):
        if not os.path.exists(path):
            sys.exit(f"herdr-theme-cache: {what} missing at {path}"
                     + (" — reinstall Herdr Colors to download themes" if what == "theme cache" else ""))

    # Select from validated copies so malformed upstream themes cannot make
    # palette generation fail at random, or inject escapes after selection.
    valid = {}
    with tempfile.TemporaryDirectory(prefix='herdr-themes-') as pool:
        for path in glob.glob(os.path.join(THEMES, '*.conf')):
            try:
                theme = parse_theme(path)
            except ValueError as exc:
                print(f'herdr-theme-cache: skipping {exc}', file=sys.stderr)
                continue
            if theme is None:
                continue
            name = os.path.basename(path)[:-5]
            valid[name] = theme
            colors = {k: v for k, v in theme.items() if k != 'palette'}
            colors.update({f'color{k}': v for k, v in theme['palette'].items()})
            with open(os.path.join(pool, name + '.conf'), 'w') as fh:
                fh.write(''.join(f'{k} {v}\n' for k, v in colors.items()))
        proc = subprocess.run(
            [sys.executable, GA, pool, str(args.count), "--max-lightness", args.max_lightness],
            capture_output=True, text=True,
        )
    if proc.returncode != 0:
        sys.exit(f"herdr-theme-cache: GA failed\n{proc.stderr.strip()}")
    names = [n for n in proc.stdout.split() if n]
    if len(names) != args.count:
        sys.exit(f"herdr-theme-cache: GA returned {len(names)} themes for {args.count} slots")

    payloads = []
    for name in names:
        theme = valid[name]
        payloads.append(osc_payload(theme))

    os.makedirs(CACHE_DIR, exist_ok=True)
    out = os.path.join(CACHE_DIR, f"{args.count}.txt")
    with open(out, "w") as fh:
        fh.write("\n".join(payloads) + "\n")
    with open(os.path.join(CACHE_DIR, "current"), "w") as fh:
        fh.write(f"{args.count}\n")

    if args.print:
        for i, name in enumerate(names, 1):
            print(f"{i}\t{name}")
    print(f"herdr-theme-cache: {args.count} slots -> {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
