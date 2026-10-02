"""Run directly: python3 tests/test_plugin.py."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check():
    # A relocated standalone plugin must work without the old toolkit or siblings.
    with tempfile.TemporaryDirectory(prefix='plugin user ') as temporary:
        home = Path(temporary)
        plugin = home / 'plugin copy'
        shutil.copytree(ROOT, plugin, ignore=shutil.ignore_patterns('.git', '__pycache__'))
        result = subprocess.run(
            ['zsh', '-fc', 'plugin=$1; source "$plugin/shell.zsh"; whence herdr-themes-build herdr-colorize', 'check', str(plugin)],
            env={**os.environ, 'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
                 'XDG_CACHE_HOME': str(home / 'cache')}, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert 'herdr-themes-build' in result.stdout
        assert 'herdr-colorize' in result.stdout

        cache = home / 'cache/herdr-pane-themes'
        cache.mkdir(parents=True)
        (cache / 'current').write_text('1\n')
        (cache / '1.txt').write_text(r'\e]11;#123456\e\\' + '\n')
        result = subprocess.run(
            ['bash', '--noprofile', '--norc', '-ic', 'source "$1/shell.bash"', 'check', str(plugin)],
            env={**os.environ, 'HOME': str(home), 'XDG_CACHE_HOME': str(home / 'cache'),
                 'HERDR_PANE_ID': 'p1'}, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert result.stdout == '\x1b]11;#123456\x1b\\', repr(result.stdout)

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
