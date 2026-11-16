#!/usr/bin/env python3
"""Check local shelf responsiveness, cover discovery and input during terminal queries."""
import argparse
from base64 import b64encode
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[1]
KITTY_REPLY = b"\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b[0n"


def write_book(path, embedded=False):
    cover = ""
    if embedded:
        image = b64encode((ROOT / "scripts/fixtures/shelf-cover.jpg").read_bytes()).decode()
        cover = (
            '<description><title-info><coverpage><image href="#cover"/>'
            '</coverpage></title-info></description>'
            f'<binary id="cover" content-type="image/jpeg">{image}</binary>'
        )
    path.write_text(f"<FictionBook>{cover}<body><section><p>{path.stem}</p></section></body></FictionBook>")


def start_shelf(binary, directory, library, rows, columns):
    env = dict(os.environ, HOME=str(directory), XDG_CONFIG_HOME=str(directory),
               XDG_CACHE_HOME=str(directory), XDG_STATE_HOME=str(directory),
               XDG_DATA_HOME=str(directory), TERM="xterm-kitty")
    for key in ("TMUX", "STY", "NO_COLOR", "TERM_PROGRAM", "KITTY_WINDOW_ID"):
        env.pop(key, None)
    child, terminal = pty.fork()
    if child == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, columns * 10, rows * 20))
        os.execve(binary, [str(binary), "--local", str(library)], env)
    return child, terminal


def check(binary, directory, listing=False):
    library = directory / "Library"
    library.mkdir()
    if listing:
        write_book(library / "Lighthouse.fb2", embedded=True)
        visible = b"Lighthouse.fb2"
    else:
        (library / "Amber").mkdir()
        visible = b"Amber/"
    config = directory / "botui"
    config.mkdir(exist_ok=True)
    (config / "config.toml").write_text(
        f'local_grid = {str(not listing).lower()}\ninterface_theme = "catppuccin-mocha"\n'
    )
    child, terminal = start_shelf(binary, directory, library, 30, 120)
    stream = bytearray()
    start = time.monotonic()
    try:
        deadline = start + 5
        while time.monotonic() < deadline:
            if not select.select([terminal], [], [], .01)[0]:
                continue
            chunk = os.read(terminal, 65536)
            stream += chunk
            # Answer palette discovery; deliberately do not answer image queries.
            if b"\x1b]11;?" in chunk:
                os.write(terminal, b"\x1b]11;rgb:2020/2020/2020\x07\x1b[?1;2c")
            if visible in stream:
                elapsed = (time.monotonic() - start) * 1000
                print(f'{"List" if listing else "Folders"}: first frame {elapsed:.1f} ms', flush=True)
                assert b"a=q" not in stream, "Navigation waited for an unnecessary image query"
                assert elapsed < 1000, "Navigation stalled before the first frame"
                if not listing:
                    return
                # Switching this same list to covers must still discover Kitty.
                os.write(terminal, b"v")
                stream.clear()
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    if select.select([terminal], [], [], .01)[0]:
                        stream += os.read(terminal, 65536)
                    if b"\x1b[5n" in stream:
                        os.write(terminal, KITTY_REPLY)
                        stream.clear()
                        break
                else:
                    raise AssertionError("Cover grid did not query image support")
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if select.select([terminal], [], [], .01)[0]:
                        stream += os.read(terminal, 65536)
                    sizes = re.findall(rb"a=[Tt][^;]*?s=(\d+),v=(\d+)", stream)
                    if any(int(w) > 20 and int(h) > 20 for w, h in sizes):
                        print("Cover grid still transfers Kitty images after lazy discovery.")
                        return
                raise AssertionError("No Kitty image after protocol discovery")
        raise AssertionError("Local library did not appear")
    finally:
        os.kill(child, signal.SIGTERM)
        os.waitpid(child, 0)
        os.close(terminal)


def enter_during_probe(binary, directory, before_reply):
    library = directory / "Library"
    library.mkdir()
    (library / "Lighthouse.fb2").write_bytes((ROOT / "examples/first-light.fb2").read_bytes())
    config = directory / "botui"
    config.mkdir()
    (config / "config.toml").write_text('local_grid = true\nzen = true\n')
    child, terminal = start_shelf(binary, directory, library, 30, 120)
    stream = bytearray()

    def receive(marker, description):
        deadline = time.monotonic() + 5
        while marker not in stream:
            remaining = deadline - time.monotonic()
            assert remaining > 0 and select.select([terminal], [], [], remaining)[0], description
            stream.extend(os.read(terminal, 65536))
        end = stream.index(marker) + len(marker)
        result = stream[:end]
        del stream[:end]
        return result

    try:
        probe = receive(b"\x1b[5n", "The cover did not query terminal capabilities")
        assert b"Lighthouse" in probe or "До первого парома".encode() in probe, "No book card before query"
        if before_reply:
            # A real key arrives while the terminal has yet to answer the probe.
            os.write(terminal, b"\r")
            time.sleep(.04)
            os.write(terminal, KITTY_REPLY)
        else:
            # A single PTY write can carry both the final reply and the next key.
            os.write(terminal, KITTY_REPLY + b"\r")
        page = receive("оставалось сорок минут.".encode(), "One Enter did not open the selected book")
        assert "Почта на пристани".encode() in page, "Expected the first chapter"
        os.write(terminal, b"q")
        returned = receive(b"Local library", "One q did not return to the library")
        assert b"Resume" not in page + returned, "A second Enter opened the reading menu"
        print(f"One Enter {'before' if before_reply else 'after'} the graphics reply opened the selected chapter.")
    finally:
        os.kill(child, signal.SIGTERM)
        os.waitpid(child, 0)
        os.close(terminal)


def covers(binary, directory, embedded=False, reopen=False):
    """A cover folder is usable before generated jackets finish rendering."""
    library = directory / "Library"
    books = library / "Books"
    books.mkdir(parents=True, exist_ok=True)
    for i in range(6):
        write_book(books / f"Book {i}.fb2", embedded)
    config = directory / "botui"
    config.mkdir(exist_ok=True)
    (config / "config.toml").write_text('local_grid = true\ninterface_theme = "catppuccin-mocha"\n')
    cached_before = {p.name:p.stat().st_mtime_ns for p in (directory/"botui/jackets-v1").glob("*.png")}
    child, terminal = start_shelf(binary, directory, library, 45, 180)

    uploaded = set()

    def frame(marker, timeout=5):
        output = bytearray()
        start = time.monotonic()
        while time.monotonic() - start < timeout:
            if not select.select([terminal], [], [], .005)[0]:
                continue
            chunk = os.read(terminal, 65536)
            output.extend(chunk)
            if b"\x1b]11;?" in chunk:
                os.write(terminal, b"\x1b]11;rgb:2020/2020/2020\x07\x1b[?1;2c")
            if b"\x1b[5n" in chunk:
                os.write(terminal, KITTY_REPLY)
            # Exclude tiny corner images; retain uploads from the first frame.
            uploads = re.findall(rb"i=(\d+),a=T[^;]*?s=(\d+),v=(\d+)", output)
            uploaded.update(i for i, w, h in uploads if int(w) > 20 and int(h) > 20)
            if marker == b"jackets":
                found = len(uploaded) >= 6
            else:
                found = marker in output
            if found and output.endswith(b"\x1b[?2026l"):
                modes = re.findall(rb"\x1b\[\?2026([hl])", output)
                assert modes == [b"h", b"l"] * (len(modes) // 2), "Unbalanced frame updates"
                return (time.monotonic() - start) * 1000
        raise AssertionError(f"No completed grid frame for {marker!r}")

    try:
        frame(b"Books/")
        os.write(terminal, b"\r")
        elapsed = frame(b"Book 0")
        print(f"Cover folder: first completed frame {elapsed:.1f} ms", flush=True)
        assert elapsed < 1000, "Jacket generation blocked entering the cover folder"
        # Generated artwork must still arrive without requiring another key.
        elapsed = frame(b"jackets", timeout=15)
        print(f"All six {'embedded covers' if embedded else 'generated jackets'} ready in {elapsed:.1f} ms", flush=True)
        for key in (b"\x1b[B", b"\x1b[A"):
            os.write(terminal, key)
            frame(b"")
        assert len(uploaded) == 6, "Selection replaced already uploaded cover images"
        os.write(terminal, b"\x7f")
        frame(b"Books/")
        os.write(terminal, b"\r")
        elapsed = frame(b"Book 0")
        print(f"Cover folder: repeat entry {elapsed:.1f} ms", flush=True)
        assert elapsed < 1000, "Returning to a cover folder blocked the event loop"
    finally:
        os.kill(child, signal.SIGTERM)
        os.waitpid(child, 0)
        os.close(terminal)

    if not embedded:
        cached_after = {p.name:p.stat().st_mtime_ns for p in (directory/"botui/jackets-v1").glob("*.png")}
        assert len(cached_after) == 6, "Generated jackets were not cached"
        if reopen:
            assert cached_before == cached_after, "Restart regenerated existing jackets"
            print("Restart reused all six cached jackets without rewriting them", flush=True)
        else:
            covers(binary, directory, embedded, reopen=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=ROOT / "target/debug/botui")
    binary = parser.parse_args().binary.resolve()
    for listing in (False, True):
        with tempfile.TemporaryDirectory(prefix="botui-local-startup-") as tmp:
            check(binary, Path(tmp), listing)
    for before_reply in (True, False):
        with tempfile.TemporaryDirectory(prefix="botui-local-enter-") as tmp:
            enter_during_probe(binary, Path(tmp), before_reply)
    for embedded in (False, True):
        with tempfile.TemporaryDirectory(prefix="botui-local-covers-") as tmp:
            covers(binary, Path(tmp), embedded)
