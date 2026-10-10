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


def run_pty(argv, env):
    pid, master = pty.fork()
    if pid == 0:
        os.execvpe(argv[0], argv, env)
    output = bytearray()
    try:
        while True:
            ready, _, _ = select.select([master], [], [], 5)
            assert ready, f'PTY command timed out: {argv}'
            try:
                chunk = os.read(master, 4096)
            except OSError:
                break
            if not chunk:
                break
            output.extend(chunk)
    finally:
        os.close(master)
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status), bytes(output)


def check():
    with tempfile.TemporaryDirectory(prefix='colors user ') as temporary:
        home = Path(temporary)
        plugin = home / 'plugin copy'
        shutil.copytree(ROOT, plugin, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__'))
        themes = home / 'themes'
        themes.mkdir()
        backgrounds = [
            '#605060', '#001030', '#382000', '#907050', '#001810',
            '#084868', '#507898', '#385838', '#986870', '#083840',
            '#203828', '#906888', '#101020', '#106070', '#300800',
            '#202848', '#605830', '#503818', '#706850', '#785058',
        ]
        ansi = ''.join(f'color{index} #808080\n' for index in range(16))
        for index, background in enumerate(backgrounds):
            (themes / f'theme {index}.conf').write_text(
                f'background {background}\nforeground #ffffff\n{ansi}')
        (themes / 'hostile.conf').write_text('background #112233\ncolor0 #123456\\e]52;injected\n')
        env = {**os.environ, 'XDG_CACHE_HOME': str(home / 'cache'),
               'HERDR_KIT_THEMES': str(themes), 'HERDR_PANE_ID': 'w1:pA'}
        result = subprocess.run(['sh', str(plugin / 'scripts/build/install.sh')],
                                cwd=home, env=env, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr
        assert 'hostile:' in result.stderr
        binary = plugin / 'bin/herdr-colors'
        manifest = (plugin / 'herdr-plugin.toml').read_text()
        assert 'command = ["./bin/herdr-themes-build"]' in manifest
        assert 'command = ["./bin/herdr-colorize"]' in manifest
        cache = home / 'cache/herdr-pane-themes'
        assert (cache / 'current').read_text().strip() == '16'
        assert len((cache / '16.txt').read_text().splitlines()) == 16

        def invoke(*args, extra_env=None, command=binary):
            invocation_env = dict(env)
            for key, value in (extra_env or {}).items():
                if value is None:
                    invocation_env.pop(key, None)
                else:
                    invocation_env[key] = value
            return subprocess.run([str(command), *args], cwd=home,
                                  env=invocation_env, capture_output=True)

        for command in ['', 'build', 'apply', 'colorize', 'detect', 'options', 'sync']:
            result = invoke(*([command] if command else []), '--help')
            assert result.returncode == 0 and b'Usage:' in result.stdout
        assert b'CIE Lab L*' in invoke('build', '--help').stdout
        for args in [('build', '0'), ('build', '-1'), ('build', 'nope'),
                     ('build', '--max-lightness', 'nan'), ('build', '--max-lightness'),
                     ('build', '--max-lightness', '4'), ('build', '--max-lightness', '101'),
                     ('build', '1', '2'), ('apply', 'p1', 'p2'), ('detect', 'extra'),
                     ('options', 'p1', 'p2'), ('options', '--bad'), ('sync', 'extra'), ('--bad',)]:
            assert invoke(*args).returncode == 2, args
        outside = invoke(
            'detect',
            extra_env={'HERDR_ENV': None, 'HERDR_PANE_ID': None, 'HERDR_BIN_PATH': None},
        )
        assert outside.returncode == 0
        outside_json = json.loads(outside.stdout)
        assert outside_json['in_herdr'] is False and outside_json['has_pane'] is False
        assert all(value is None for value in outside_json['neighbors'].values())
        disabled = invoke(
            'detect',
            extra_env={'HERDR_ENV': '0', 'HERDR_PANE_ID': 'w1:p1', 'HERDR_BIN_PATH': ''},
        )
        assert disabled.returncode == 0
        assert json.loads(disabled.stdout)['in_herdr'] is False
        if os.supports_bytes_environ:
            non_utf_env = {
                os.fsencode(key): os.fsencode(value)
                for key, value in env.items()
            }
            non_utf_env[b'HERDR_ENV'] = b'1'
            non_utf_env[b'HERDR_PANE_ID'] = b'w1:p\xff'
            non_utf = subprocess.run(
                [os.fsencode(binary), b'detect'],
                env=non_utf_env,
                capture_output=True,
            )
            assert non_utf.returncode == 1
            assert 'valid UTF-8' in json.loads(non_utf.stdout)['error']
        before = (cache / '16.txt').read_bytes()
        assert invoke('build', '999').returncode == 1
        assert (cache / '16.txt').read_bytes() == before
        assert invoke('apply', '--quiet', extra_env={'PATH': ''}).stdout.startswith(b'\x1b]11;#')
        for shell in ['bash', 'zsh']:
            result = subprocess.run([shell, '-fic', 'source "$1/shell.$2"', 'check', str(plugin), shell],
                                    env=env, capture_output=True)
            assert result.returncode == 0, result.stderr
            assert b'\x1b]' not in result.stdout, result.stdout
            code, output = run_pty(
                [shell, '-fic', 'source "$1/shell.$2"', 'check', str(plugin), shell],
                env,
            )
            assert code == 0 and b'\x1b]11;#' in output, output
            relative = os.path.relpath(plugin, home)
            result = subprocess.run(
                [shell, '-fic', 'source "$1/shell.$2"; test "$_HERDR_COLORS_ROOT" = "$3"',
                 'check', str(plugin), shell, str(plugin.resolve())],
                cwd=home,
                env={**env, 'HERDR_PLUGIN_ROOT': relative, 'CDPATH': str(home.parent)},
                capture_output=True,
            )
            assert result.returncode == 0, result.stderr
        # An old cache remains readable and is preserved during sync/upgrades.
        (cache / 'current').write_text('1\n')
        (cache / '1.txt').write_text(r'\e]11;#123456\e\\' + '\n')
        assert invoke('sync').returncode == 0
        assert (cache / 'current').read_text() == '1\n'
        assert invoke('apply', 'w1:pA').stdout == b'\x1b]11;#123456\x1b\\'
        # A concurrent Git lock is retried instead of making sync fail spuriously.
        git_cache = home / 'git cache'
        (git_cache / 'kitty-themes/.git').mkdir(parents=True)
        shutil.copytree(cache, git_cache / 'herdr-pane-themes')
        fake_bin = home / 'fake git bin'
        fake_bin.mkdir()
        lock_state = home / 'git lock cleared'
        fake_git = fake_bin / 'git'
        fake_git.write_text('''#!/bin/sh
if [ ! -e "$TEST_GIT_LOCK_STATE" ]; then
    touch "$TEST_GIT_LOCK_STATE"
    echo "fatal: Unable to create index.lock: File exists" >&2
    exit 1
fi
''')
        fake_git.chmod(0o755)
        git_env = {
            'HERDR_KIT_THEMES': None,
            'XDG_CACHE_HOME': str(git_cache),
            'PATH': f'{fake_bin}:{os.environ["PATH"]}',
            'TEST_GIT_LOCK_STATE': str(lock_state),
        }
        assert invoke('sync', extra_env=git_env).returncode == 0
        # Sync validates every slot, not only the palette used by p1.
        (cache / 'current').write_text('2\n')
        (cache / '2.txt').write_text(
            '\\e]11;#123456\\e\\\\\n\\e]52;#123456\\e\\\\\n')
        assert invoke('sync').returncode == 0
        assert (cache / 'current').read_text() == '16\n'
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
        relative_cache_env = {'XDG_CACHE_HOME': 'relative-cache', 'HOME': str(home)}
        assert invoke('build', '1', extra_env=relative_cache_env).returncode == 0
        assert (home / '.cache/herdr-pane-themes/1.txt').is_file()
        result = subprocess.run(
            [os.fsencode(binary), b'apply', b'p\xff'], env=env, capture_output=True)
        assert result.returncode == 2 and b'valid UTF-8' in result.stderr
        assert invoke('build', '4').returncode == 0

        # Paint a real terminal through mocked Herdr JSON, but real ps and PTY I/O.
        state = home / 'herdr.json'
        mock = home / 'herdr mock'
        mock.write_text('''#!/usr/bin/env python3
import json, os, sys
state = json.load(open(os.environ['TEST_HERDR_STATE']))
if state.get('fail'):
    print('server unavailable', file=sys.stderr)
    sys.exit(7)
args = sys.argv[1:]
layout = state.get('layout') or {
    'area': {'x': 0, 'y': 0, 'width': 20, 'height': 20},
    'panes': [
        {'pane_id': 'w1:p1', 'rect': {'x': 0, 'y': 10, 'width': 10, 'height': 10}},
        {'pane_id': 'w1:p2', 'rect': {'x': 0, 'y': 0, 'width': 10, 'height': 10}},
        {'pane_id': 'w1:p3', 'rect': {'x': 10, 'y': 10, 'width': 10, 'height': 10}},
    ]
}
if state.get('malformed') == 'area':
    layout['area'] = None
elif state.get('malformed') == 'coordinate':
    layout['area']['x'] = -1
if args == ['pane', 'list']:
    print(json.dumps({'result': {'panes': [{'pane_id': 'w1:p1'}, {'pane_id': 'w1:p2'}]}}))
elif args == ['pane', 'current', '--pane', 'w1:p1']:
    pane = {
        'pane_id': 'w1:p1', 'terminal_id': 'term_test', 'workspace_id': 'w1',
        'tab_id': 'w1:t1', 'cwd': '/tmp/project'
    }
    print(json.dumps({'result': {
        'pane': None if state.get('malformed') == 'pane' else pane
    }}))
elif args[:3] == ['pane', 'layout', '--pane'] and len(args) == 4:
    print(json.dumps({'result': {'layout': layout}}))
elif args[:2] == ['pane', 'neighbor']:
    assert args[2:4] == ['--pane', 'w1:p1']
    assert args[4] == '--direction' and len(args) == 6
    direction = args[-1]
    neighbor = {'up': 'w1:p2', 'right': 'w1:p3'}.get(direction)
    value = {'pane_id': 'w1:p1', 'direction': direction, 'layout': layout}
    if neighbor:
        value['neighbor_pane_id'] = neighbor
    if direction == 'up':
        value['layout'] = json.loads(json.dumps(layout))
        value['layout']['panes'][1]['rect']['x'] = 1
    if state.get('malformed') == 'neighbor' and direction == 'up':
        value['neighbor_pane_id'] = 7
    print(json.dumps({'result': {'neighbor': value}}))
else:
    assert args[:3] == ['pane', 'process-info', '--pane'] and len(args) == 4
    pid = state['pid'] if args[-1] == 'w1:p1' else None
    print(json.dumps({'result': {'process_info': {'shell_pid': pid}}}))
''')
        mock.chmod(0o755)
        pid, master = pty.fork()
        if pid == 0:
            os.execvp('sleep', ['sleep', '60'])
        try:
            state.write_text(json.dumps({'pid': pid}))
            fake = {
                'HERDR_BIN_PATH': str(mock),
                'HERDR_ENV': '1',
                'TEST_HERDR_STATE': str(state),
            }
            detected = invoke(
                'detect',
                extra_env={**fake, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'w1:p1'},
            )
            assert detected.returncode == 0, detected.stderr
            detected_json = json.loads(detected.stdout)
            assert detected_json['in_herdr'] is True
            assert detected_json['pane']['terminal_id'] == 'term_test'
            assert detected_json['location']['y'] == 10
            assert detected_json['neighbors']['up']['pane_id'] == 'w1:p2'
            assert detected_json['neighbors']['up']['location']['x'] == 1
            assert detected_json['neighbors']['right']['pane_id'] == 'w1:p3'
            assert detected_json['neighbors']['left'] is None
            assert detected_json['neighbors']['down'] is None
            for malformed in ['pane', 'area', 'coordinate', 'neighbor']:
                state.write_text(json.dumps({'pid': pid, 'malformed': malformed}))
                malformed_result = invoke(
                    'detect',
                    extra_env={**fake, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'w1:p1'},
                )
                assert malformed_result.returncode == 1
                assert json.loads(malformed_result.stdout)['error']
            state.write_text(json.dumps({'pid': pid}))
            options = invoke('options', 'w1:p1', extra_env=fake)
            assert options.returncode == 0, options.stderr
            options_json = json.loads(options.stdout)
            assert options_json['strategy'] == 'genetic-layout'
            assert len(options_json['options']) == 4
            assert len(options_json['assignment']) == 3
            assert len(options_json['touching_neighbors']) == 2
            assert sum(option['selected'] for option in options_json['options']) == 1
            repeated = json.loads(invoke('options', 'w1:p1', extra_env=fake).stdout)
            assert repeated['selected_slot'] == options_json['selected_slot']
            applied = invoke('apply', 'w1:p1', extra_env=fake)
            expected = b'\x1b]11;' + options_json['selected_background'].encode() + b'\x1b\\'
            assert applied.returncode == 0 and applied.stdout.startswith(expected)

            def row_layout(ids):
                return {
                    'area': {'x': 0, 'y': 0, 'width': len(ids) * 10, 'height': 10},
                    'panes': [
                        {
                            'pane_id': pane_id,
                            'rect': {'x': index * 10, 'y': 0, 'width': 10, 'height': 10},
                        }
                        for index, pane_id in enumerate(ids)
                    ],
                }

            for ids in [['w1:p1'], ['w1:p1', 'w1:p2', 'w1:p3', 'w1:p4', 'w1:p5'],
                        ['w1:p1', 'w1:p5']]:
                state.write_text(json.dumps({'pid': pid, 'layout': row_layout(ids)}))
                first = invoke('options', 'w1:p1', extra_env=fake)
                second = invoke('options', 'w1:p1', extra_env=fake)
                assert first.returncode == 0 and second.returncode == 0
                first_json = json.loads(first.stdout)
                assert first_json == json.loads(second.stdout)
                assert len(first_json['assignment']) == len(ids)
                if len(ids) == 1:
                    assert all(option['score'] is None for option in first_json['options'])
                else:
                    assigned_slots = [
                        assignment['slot'] for assignment in first_json['assignment']
                    ]
                    assert all(a != b for a, b in zip(assigned_slots, assigned_slots[1:]))

            duplicate_layout = row_layout(['w1:p1', 'w1:p1'])
            state.write_text(json.dumps({'pid': pid, 'layout': duplicate_layout}))
            malformed_assignment = invoke('options', 'w1:p1', extra_env=fake)
            assert malformed_assignment.returncode == 1
            assert 'duplicate pane ID' in json.loads(malformed_assignment.stdout)['error']
            fallback = invoke('apply', 'w1:p1', '--quiet', extra_env=fake)
            base_background = options_json['options'][0]['background'].encode()
            assert fallback.returncode == 0
            assert fallback.stdout.startswith(b'\x1b]11;' + base_background + b'\x1b\\')

            state.write_text(json.dumps({'pid': pid}))
            result = invoke('w1:p1', extra_env=fake, command=plugin / 'bin/herdr-colorize')
            assert result.returncode == 0, result.stderr
            assert select.select([master], [], [], 5)[0], 'No color bytes reached the terminal'
            assert os.read(master, 4096).startswith(b'\x1b]11;#')
            result = invoke('colorize', extra_env=fake)
            assert result.returncode == 1 and b'no shell PID' in result.stderr, result.stderr
            herdr_on_path = home / 'herdr'
            shutil.copy2(mock, herdr_on_path)
            empty_override = {
                'HERDR_BIN_PATH': '',
                'TEST_HERDR_STATE': str(state),
                'PATH': f'{home}:{os.environ["PATH"]}',
            }
            assert invoke('w1:p1', extra_env=empty_override,
                          command=plugin / 'bin/herdr-colorize').returncode == 0
            state.write_text(json.dumps({'fail': True}))
            fallback = invoke('apply', 'w1:p1', '--quiet', extra_env=fake)
            assert fallback.returncode == 0 and fallback.stdout.startswith(b'\x1b]11;#')
            assert not fallback.stderr
            failed_options = invoke('options', 'w1:p1', extra_env=fake)
            assert failed_options.returncode == 1
            assert 'server unavailable' in json.loads(failed_options.stdout)['error']
            failed_detection = invoke(
                'detect',
                extra_env={**fake, 'HERDR_ENV': '1', 'HERDR_PANE_ID': 'w1:p1'},
            )
            assert failed_detection.returncode == 1
            assert 'server unavailable' in json.loads(failed_detection.stdout)['error']
            assert invoke('colorize', extra_env=fake).returncode == 1
        finally:
            os.kill(pid, 15)
            os.waitpid(pid, 0)
            os.close(master)
        print('Rust build, relocated install, Bash/Zsh startup, legacy caches, CLI errors, and PTY repaint passed.')


if __name__ == '__main__':
    check()
