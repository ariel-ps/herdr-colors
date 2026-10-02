"""Integration check: python3 tests/test_plugin.py (Python is test-only)."""
import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check():
    with tempfile.TemporaryDirectory(prefix='colors user ') as temporary:
        home = Path(temporary)
        plugin = home / 'plugin copy'
        shutil.copytree(ROOT, plugin, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__'))
        themes = home / 'themes'
        themes.mkdir()
        for index in range(20):
            (themes / f'theme {index}.conf').write_text(
                f'background #{index + 10:02x}2020\nforeground #ffffff\n')
        (themes / 'hostile.conf').write_text('background #112233\ncolor0 #123456\\e]52;injected\n')
        env = {**os.environ, 'XDG_CACHE_HOME': str(home / 'cache'),
               'HERDR_KIT_THEMES': str(themes), 'HERDR_PANE_ID': 'w1:pA'}
        result = subprocess.run(['sh', str(plugin / 'scripts/build/install.sh')],
                                cwd=home, env=env, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert 'hostile:' in result.stderr
        binary = plugin / 'bin/herdr-colors'
        cache = home / 'cache/herdr-pane-themes'
        assert (cache / 'current').read_text().strip() == '16'
        assert len((cache / '16.txt').read_text().splitlines()) == 16

        def invoke(*args, extra_env=None, command=binary):
            return subprocess.run([str(command), *args], cwd=home,
                                  env={**env, **(extra_env or {})}, capture_output=True)

        for command in ['', 'build', 'apply', 'colorize', 'sync']:
            result = invoke(*([command] if command else []), '--help')
            assert result.returncode == 0 and b'Usage:' in result.stdout
        for args in [('build', '0'), ('build', '-1'), ('build', 'nope'),
                     ('build', '--max-lightness', 'nan'), ('build', '--max-lightness'),
                     ('build', '1', '2'), ('apply', 'p1', 'p2'), ('sync', 'extra'), ('--bad',)]:
            assert invoke(*args).returncode == 2, args
        before = (cache / '16.txt').read_bytes()
        assert invoke('build', '999').returncode == 1
        assert (cache / '16.txt').read_bytes() == before
        assert invoke('apply', '--quiet', extra_env={'PATH': ''}).stdout.startswith(b'\x1b]11;#')
        for shell in ['bash', 'zsh']:
            result = subprocess.run([shell, '-fic', 'source "$1/shell.$2"', 'check', str(plugin), shell],
                                    env=env, capture_output=True)
            assert result.returncode == 0, result.stderr
            assert result.stdout.startswith(b'\x1b]11;#'), result.stdout
        # An old cache remains readable and is preserved during sync/upgrades.
        (cache / 'current').write_text('1\n')
        (cache / '1.txt').write_text(r'\e]11;#123456\e\\' + '\n')
        assert invoke('sync').returncode == 0
        assert (cache / 'current').read_text() == '1\n'
        assert invoke('apply', 'w1:pA').stdout == b'\x1b]11;#123456\x1b\\'
        for invalid in ['0', '-1', 'not-a-number']:
            (cache / 'current').write_text(invalid)
            result = invoke('apply', '--quiet')
            assert result.returncode == 1 and not result.stdout and not result.stderr
        (cache / 'current').write_text('1\n')
        (cache / '1.txt').write_text(r'\e]52;#123456\e\\' + '\n')
        assert invoke('apply').returncode == 1
        (cache / '1.txt').write_text(r'\e]11;#123456\e\\' + '\n')
        result = invoke('1', '--print', command=plugin / 'bin/herdr-themes-build')
        assert result.returncode == 0 and b'1\ttheme ' in result.stdout, result.stderr

        # Paint a real terminal through mocked Herdr JSON, but real ps and PTY I/O.
        state = home / 'herdr.json'
        mock = home / 'herdr mock'
        mock.write_text('''#!/usr/bin/env python3
import json, os, sys
state = json.load(open(os.environ['TEST_HERDR_STATE']))
if state.get('fail'):
    print('server unavailable', file=sys.stderr)
    sys.exit(7)
if sys.argv[1:3] == ['pane', 'list']:
    print(json.dumps({'result': {'panes': [{'pane_id': 'w1:p1'}, {'pane_id': 'w1:p2'}]}}))
else:
    pid = state['pid'] if sys.argv[-1] == 'w1:p1' else None
    print(json.dumps({'result': {'process_info': {'shell_pid': pid}}}))
''')
        mock.chmod(0o755)
        pid, master = pty.fork()
        if pid == 0:
            os.execvp('sleep', ['sleep', '60'])
        try:
            state.write_text(json.dumps({'pid': pid}))
            fake = {'HERDR_BIN_PATH': str(mock), 'TEST_HERDR_STATE': str(state)}
            result = invoke('w1:p1', extra_env=fake, command=plugin / 'bin/herdr-colorize')
            assert result.returncode == 0, result.stderr
            assert select.select([master], [], [], 5)[0], 'No color bytes reached the terminal'
            assert os.read(master, 4096).startswith(b'\x1b]11;#')
            result = invoke('colorize', extra_env=fake)
            assert result.returncode == 1 and b'no shell PID' in result.stderr, result.stderr
            state.write_text(json.dumps({'fail': True}))
            assert invoke('colorize', extra_env=fake).returncode == 1
        finally:
            os.kill(pid, 15)
            os.waitpid(pid, 0)
            os.close(master)
        print('Rust build, relocated install, Bash/Zsh startup, legacy caches, CLI errors, and PTY repaint passed.')


if __name__ == '__main__':
    check()
