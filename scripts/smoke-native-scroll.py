#!/usr/bin/env python3
"""Check wide Kitty PDF scrolling in a private PTY, without desktop windows.

The simulated terminal accepts inline images, without a shared-memory shortcut.
Optional binary arguments allow sequential before/after measurements.
"""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('sources', ROOT / 'scripts/smoke-sources.py')
sources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sources)


def check(binary, directory):
    (directory / 'botui').mkdir()
    (directory / 'botui/config.toml').write_text('reading_layout="scroll"\nzen=true\n')
    book = directory / 'test.pdf'
    book.write_bytes(sources.pdf(True, 300, 440, (.3, .8, .5, .4, .6, .7)))
    env = dict(os.environ, XDG_CONFIG_HOME=str(directory), XDG_STATE_HOME=str(directory),
               XDG_CACHE_HOME=str(directory), TERM='xterm-kitty')
    for key in ('TMUX', 'STY', 'NO_COLOR', 'TERM_PROGRAM', 'KITTY_WINDOW_ID'):
        env.pop(key, None)
    child, terminal = pty.fork()
    if child == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 45, 180, 2160, 1080))
        os.execve(str(binary), [str(binary), str(book)], env)

    def collect(timeout=3):
        data = bytearray()
        start = last = time.monotonic()
        while time.monotonic() - start < timeout:
            if select.select([terminal], [], [], .005)[0]:
                chunk = os.read(terminal, 262144)
                data.extend(chunk)
                last = time.monotonic()
                if b'\x1b]11;?' in chunk:
                    os.write(terminal, b'\x1b]11;rgb:2020/2020/2020\x07\x1b[?1;2c')
                if b'\x1b[5n' in chunk:
                    os.write(terminal, b'\x1b_Gi=31;OK\x1b\\\x1b[6;24;12t\x1b[0n')
            elif data.endswith(b'\x1b[?2026l') and time.monotonic() - last > .025:
                uploads = len(re.findall(rb'a=[Tt][^;]*s=\d+', data))
                return round((last - start) * 1000, 1), len(data), uploads
        if timeout >= 3:
            raise AssertionError('No completed frame while scrolling')
        return 0, len(data), 0

    try:
        collect(10)
        time.sleep(1)
        collect(.2)
        forward = []
        for step in range(180):
            os.write(terminal, b'\x1b[B')
            forward.append((step, *collect()))
        uploads = sum(row[3] for row in forward)
        transferred = sum(row[2] for row in forward)
        assert uploads <= 4, f'Cached pages were re-uploaded {uploads} times'
        assert transferred < 60_000_000, 'Scrolling repeatedly transmitted viewport rasters'
        backward = []
        for _ in range(150):
            os.write(terminal, b'\x1b[A')
            backward.append(collect())
        assert sum(row[2] for row in backward) == 0, 'Returning uploaded cached pages again'
        print(json.dumps({
            'binary': str(binary), 'slow': [row for row in forward if row[1] > 40],
            'max_ms': max(row[1] for row in forward),
            'median_ms': sorted(row[1] for row in forward)[len(forward) // 2],
            'uploads': uploads, 'bytes': transferred,
        }), flush=True)
    finally:
        os.kill(child, signal.SIGTERM)
        os.waitpid(child, 0)
        os.close(terminal)


if __name__ == '__main__':
    for executable in sys.argv[1:] or [str(ROOT / 'target/debug/botui')]:
        with tempfile.TemporaryDirectory(prefix='botui-native-scroll-') as temp:
            check(Path(executable).resolve(), Path(temp))
