"""Run directly: python3 tests/test_plugin.py."""
import os
import re
import runpy
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check():
    manifest = (ROOT / 'herdr-plugin.toml').read_text()
    assert re.search(r'(?m)^id = "dev\.ariel\.herdr-colors"$', manifest)
    assert re.search(r'(?m)^command = \["sh", "\./scripts/build/sync-themes\.sh"\]$', manifest)
    assert manifest.count('[[actions]]') == 2
    assert re.findall(r'(?m)^id = "(themes|colorize)"$', manifest) == ['themes', 'colorize']

    # A relocated standalone plugin must work without the old toolkit or siblings.
    with tempfile.TemporaryDirectory(prefix='plugin user ') as temporary:
        home = Path(temporary)
        parse = runpy.run_path(str(ROOT / 'libexec/theme-cache.py'))['parse_theme']
        theme = home / 'theme.conf'
        theme.write_text('background #123456\nforeground #abcdef\ncolor0 #ABCDEF\n')
        assert parse(theme)['palette'][0] == '#ABCDEF'
        for content in [r'background #123456\e]52;injected',
                        'background #123456\x1b]52;injected',
                        'background #123456\nbackground #abcdef',
                        'background #123456\ncolor0 #%s%s%s',
                        'background #123456\nforeground #abc']:
            theme.write_text(content + '\n')
            try:
                parse(theme)
            except ValueError:
                pass
            else:
                raise AssertionError(f'Unsafe color accepted: {content!r}')
        plugin = home / 'plugin copy'
        shutil.copytree(ROOT, plugin, ignore=shutil.ignore_patterns('.git', '__pycache__'))
        result = subprocess.run(
            ['zsh', '-fc', 'plugin=$1; source "$plugin/shell.zsh"; whence herdr-themes-build herdr-colorize', 'check', str(plugin)],
            env={**os.environ, 'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
                 'XDG_CACHE_HOME': str(home / 'cache')}, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert 'herdr-themes-build' in result.stdout
        assert 'herdr-colorize' in result.stdout

        # The manifest build must prepare usable palettes on a fresh install.
        # Stub only the network command; run the real palette generator.
        tools = home / 'tools'
        tools.mkdir()
        git = tools / 'git'
        git.write_text('#!/bin/sh\nexit 0\n')
        git.chmod(0o755)
        themes = home / 'themes'
        themes.mkdir()
        for index in range(20):
            (themes / f'theme{index}.conf').write_text(
                f'background #{index + 10:02x}2020\nforeground #ffffff\n')
        (themes / 'hostile.conf').write_text('background #112233\ncolor0 #123456\\e]52;injected\n')
        env = {**os.environ, 'HOME': str(home), 'XDG_CACHE_HOME': str(home / 'cache'),
               'HERDR_KIT_THEMES': str(themes), 'HERDR_PANE_ID': 'w1:pA',
               'PATH': str(tools) + ':' + os.environ['PATH']}
        result = subprocess.run(['sh', str(plugin / 'scripts/build/sync-themes.sh')], cwd=home,
                                env=env, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert 'skipping' in result.stderr and 'hostile.conf' in result.stderr, result.stderr
        cache = home / 'cache/herdr-pane-themes'
        assert (cache / 'current').read_text().strip() == '16'
        assert len((cache / '16.txt').read_text().splitlines()) == 16
        for shell, integration in [('bash', 'shell.bash'), ('zsh', 'shell.zsh')]:
            result = subprocess.run([shell, '-fic', 'source "$1/$2"', 'check', str(plugin), integration],
                                    env=env, text=True, capture_output=True)
            assert result.returncode == 0, result.stderr
            assert result.stdout.startswith('\x1b]11;#'), repr(result.stdout)

        for invalid in ['0', '-1', 'not-a-number']:
            (cache / 'current').write_text(invalid + '\n')
            result = subprocess.run(['zsh', '-fc', 'source "$1/shell.zsh"; __herdr_payload_for w1:p1',
                                     'check', str(plugin)], env=env, text=True, capture_output=True)
            assert result.returncode == 1 and not result.stdout and not result.stderr, result
        result = subprocess.run(['python3', str(plugin / 'libexec/theme-cache.py'), '0'],
                                env=env, text=True, capture_output=True)
        assert result.returncode == 2 and 'at least 1' in result.stderr

        (cache / 'current').write_text('1\n')
        (cache / '1.txt').write_text(r'\e]11;#123456\e\\' + '\n')
        result = subprocess.run(['sh', str(plugin / 'scripts/build/sync-themes.sh')], cwd=home,
                                env=env, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert (cache / 'current').read_text() == '1\n', 'Upgrade replaced chosen palettes'
        result = subprocess.run(
            ['bash', '--noprofile', '--norc', '-ic', 'source "$1/shell.bash"', 'check', str(plugin)],
            env={**os.environ, 'HOME': str(home), 'XDG_CACHE_HOME': str(home / 'cache'),
                 'HERDR_PANE_ID': 'p1'}, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert result.stdout == '\x1b]11;#123456\x1b\\', repr(result.stdout)

        (cache / 'current').unlink()
        result = subprocess.run(['zsh', '-fc',
                                 'source "$1/shell.zsh"; __herdr_colors_ready() { return 0; }; herdr-colorize w1:p1',
                                 'check', str(plugin)], env=env, text=True, capture_output=True)
        assert result.returncode == 1 and 'Run herdr-themes-build' in result.stderr, result

        # Verify bash forwards literal arguments, cwd, and failures to the implementation.
        commands = ["herdr-themes-build","herdr-colorize"]
        (plugin / 'shell.zsh').write_text('\n'.join(
            name + '() { printf "%s\\n" "$PWD" "${HERDR_AGENT_ARGS:-}" "$@"; return 7; }'
            for name in commands))
        arguments = ['two words', '$(touch unexpected)', '', '--option']
        for command in commands:
            result = subprocess.run(
                ['bash', '--noprofile', '--norc', '-c',
                 'source "$1/shell.bash"; shift; HERDR_AGENT_ARGS="two flags"; "$@"',
                 'check', str(plugin), command, *arguments], cwd=home,
                env={**os.environ, 'HOME': str(home)}, text=True, capture_output=True)
            assert result.returncode == 7, result.stderr
            assert result.stdout.splitlines() == [str(home.resolve()), os.environ.get('HERDR_AGENT_ARGS', ''), *arguments], result.stdout
        assert not (home / 'unexpected').exists()


if __name__ == '__main__':
    check()
