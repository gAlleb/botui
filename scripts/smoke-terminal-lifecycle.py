#!/usr/bin/env python3
"""Teardown must finish owned writers before a smoke profile is removed/reused."""
import importlib.util
import os
from pathlib import Path
import select
import sys
import tempfile
import threading
import time

spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)

WRITER = '''import os, signal, subprocess, sys, time
from pathlib import Path
root, control = map(Path, sys.argv[1:3])
role = sys.argv[3]
def stop(sig, frame):
    if role == 'parent':
        child.send_signal(signal.SIGHUP)
    (control / (role + '-hup')).touch()
    while not (control / (role + '-release')).exists():
        time.sleep(.01)
    (root / (role + '-late')).write_text('last write before exit')
    os._exit(0)
signal.signal(signal.SIGHUP, stop)
(control / (role + '-pid')).write_text(str(os.getpid()))
(control / (role + '-ready')).touch()
if role == 'parent':
    child = subprocess.Popen([sys.executable, __file__, str(root), str(control), 'child'],
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
    while not (control / 'child-ready').exists():
        time.sleep(.01)
    print('READY', flush=True)
while True:
    signal.pause()
'''


def wait(predicate, label):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.01)
    raise AssertionError(label)


def exited(fd):
    return bool(select.select([fd], [], [], 0)[0])


with tempfile.TemporaryDirectory(prefix='botui-terminal-lifecycle-') as tmp:
    control = Path(tmp)
    writer = control / 'writer.py'
    writer.write_text(WRITER)
    with tempfile.TemporaryDirectory(prefix='profile-', dir=control) as profile:
        root = Path(profile)
        term = s.Terminal(root, Path(sys.executable),
                          args=[str(writer), str(root), str(control), 'parent'])
        term.see('READY')
        parent = os.pidfd_open(int((control / 'parent-pid').read_text()))
        child = os.pidfd_open(int((control / 'child-pid').read_text()))
        done = threading.Event()
        errors = []

        def close():
            try:
                term.run('kill-server', required=False)
            except BaseException as error:
                errors.append(error)
            finally:
                done.set()

        closing = threading.Thread(target=close, daemon=True)
        closing.start()
        try:
            wait(lambda: all((control / (role + '-hup')).exists()
                             for role in ('parent', 'child')), 'owned writers did not receive HUP')
            # This tests a held gate, not a speed requirement for the reader.
            assert not done.wait(.1), 'teardown returned while both writers were still alive'
            (control / 'parent-release').touch()
            wait(lambda: exited(parent), 'parent did not exit after its gate was released')
            assert not exited(child), 'child should still be waiting for its separate gate'
            assert not done.wait(.1), 'teardown waited for the parent but lost its child'
            (control / 'child-release').touch()
            assert done.wait(5), 'teardown did not finish after both writers exited'
            assert not errors, errors
            assert exited(child), 'teardown returned before the child exited'
            for role in ('parent', 'child'):
                assert (root / (role + '-late')).read_text() == 'last write before exit'
            term.run('kill-server', required=False)  # Closing twice is safe.
        finally:
            for role in ('parent', 'child'):
                (control / (role + '-release')).touch()
            closing.join(6)
            wait(lambda: exited(parent) and exited(child), 'fixture writers left running')
            os.close(parent)
            os.close(child)

        # Reuse the profile and ensure successful teardown preserves the actual
        # test failure instead of replacing it with a cleanup exception.
        restarted = s.Terminal(root, Path(sys.executable), args=[
            '-c', "import signal; print('RESTARTED', flush=True); signal.pause()"])
        try:
            try:
                restarted.see('RESTARTED')
                raise AssertionError('original functional assertion')
            finally:
                restarted.run('kill-server', required=False)
        except AssertionError as error:
            assert str(error) == 'original functional assertion', error
    assert not root.exists(), 'temporary profile was not removed'

print('PASS terminal teardown waits for parent and child writers, repeated close, restart and original failure')
