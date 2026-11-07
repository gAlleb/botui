#!/usr/bin/env python3
"""A delayed, unchanged OSC 11 reply must not block an in-flight catalog fetch."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import tempfile
import termios
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

COLOR = b"\x1b]11;rgb:2020/2020/2020\x07"
QUERY = re.compile(rb"\x1b\]11;\?(?:\x07|\x1b\\)")
VISIBLE = b"IncomingShelf"


def check(binary, directory):
    release = threading.Event()

    class Feed(BaseHTTPRequestHandler):
        def do_GET(self):
            release.wait(4)
            body = json.dumps({"metadata": {"title": "Local OPDS"},
                               "navigation": [{"title": VISIBLE.decode(), "href": "/booklist",
                                               "type": "application/opds+json"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/opds+json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    source = ThreadingHTTPServer(("127.0.0.1", 0), Feed)
    form = directory / "botui"
    form.mkdir()
    (form / "config.toml").write_text('opaque_background = true\ninterface_theme = "terminal"\n')
    env = dict(os.environ, HOME=str(directory), XDG_CONFIG_HOME=str(directory),
               XDG_CACHE_HOME=str(directory), XDG_STATE_HOME=str(directory), TERM="xterm-256color")
    for k in ("TMUX", "STY", "NO_COLOR", "TERM_PROGRAM", "KITTY_WINDOW_ID"):
        env.pop(k, None)
    child, terminal = pty.fork()
    if child == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 142, 0, 0))
        os.execve(binary, [str(binary), "--opds", f"http://127.0.0.1:{source.server_port}/"], env)

    worker = threading.Thread(target=source.serve_forever, daemon=True)
    worker.start()
    stream = bytearray()
    queries = 0
    submitted = False
    delivered = False
    try:
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if select.select([terminal], [], [], 0.05)[0]:
                stream += os.read(terminal, 65536)
            count = len(QUERY.findall(stream))
            if queries == 0 and count:
                # Background discovery also asks for primary device attributes.
                os.write(terminal, COLOR + b"\x1b[?1;2c")
            queries = count
            if not submitted and b"OPDS catalog" in stream and count >= 2:
                # Hold the first live palette reply until the HTTP job is waiting.
                os.write(terminal, b"\t\t\r")
                submitted = True
            if not delivered and b"Opening OPDS catalog" in stream:
                os.write(terminal, COLOR)
                time.sleep(0.15)
                release.set()
                delivered = True
            if delivered and VISIBLE in stream:
                print("Unchanged palette reply and delayed catalog completion repaint without a key.")
                return
        raise AssertionError(f"Timed out: palette queries={queries}, submitted={submitted}, delivered={delivered}")
    finally:
        release.set()
        os.kill(child, signal.SIGTERM)
        os.waitpid(child, 0)
        os.close(terminal)
        source.shutdown()
        source.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=Path("target/debug/botui"))
    with tempfile.TemporaryDirectory(prefix="botui-osc11-") as tmp:
        check(parser.parse_args().binary.resolve(), Path(tmp))
