#!/usr/bin/env python3
"""Exercise real SGR drags in the BookOrbit library, text reader, and scanned pages."""
import importlib.util
from pathlib import Path
import re
import tempfile
import threading
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/botui"
spec = importlib.util.spec_from_file_location("sources", ROOT / "scripts/smoke-sources.py")
sources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sources)
shelf = sources.shelf


def mouse(term, code, column, row=8, end="M"):
    sequence = f"\033[<{code};{column + 1};{row + 1}{end}"
    term.keys("-H", *[f"{byte:02x}" for byte in sequence.encode()])


def at_width(term, cells):
    return term.wait(f"Grip at column {cells}", lambda f: any(
        line.find("↔") == cells for line in f.splitlines()
    ))


def size(term, width):
    term.run("resize-window", "-t", "shelf", "-x", str(width), "-y", "32")


def config(directory):
    return tomllib.loads((directory / "config/botui/config.toml").read_text())


def change_width(term, directory, field, start, end, cancel=False):
    """Test both the visible grip and panel border, then release off the original edge."""
    old = config(directory).get(field)
    edge = -1 if cancel else 0
    mouse(term, 0, start + edge)
    mouse(term, 32, end + edge, 10)
    at_width(term, end)
    assert config(directory).get(field) == old, "Preview was saved before mouse release"
    if cancel:
        term.keys("Escape")
        at_width(term, start)
        mouse(term, 0, end + edge, 5, "m")
        assert config(directory).get(field) == old
    else:
        mouse(term, 0, end + edge, 5, "m")
        term.wait("Committed width", lambda _: config(directory).get(field) == end)


def exercise(directory, server):
    url = f"http://127.0.0.1:{server.server_port}/reader"
    term = shelf.Terminal(directory, BINARY, url)
    try:
        shelf.login(term, url)
        size(term, 160)
        at_width(term, 32)
        change_width(term, directory, "sidebar_width", 32, 75, cancel=True)
        change_width(term, directory, "sidebar_width", 32, 75)
        size(term, 123)
        at_width(term, 62)  # Keep 60 cells available for the book cards.
        assert config(directory)["sidebar_width"] == 75
        size(term, 160)
        at_width(term, 75)
        term.see("[V List]")
        assert not any(path.endswith("/serve") for _, path, _ in server.requests)
        term.keys("q")
        term.see("Choose a library")
        term.keys("q")
    finally:
        term.run("kill-server", required=False)

    # The library width is separate from the width shared by both readers.
    text = directory / "chapters.fb2"
    text.write_text("<FictionBook><body><section><title><p>Letters</p></title>"
                    + "".join(f"<p>Passage {i:03}. Across the harbour.</p>" for i in range(200))
                    + "</section></body></FictionBook>")
    term = shelf.Terminal(directory, BINARY, args=[str(text)])
    try:
        at_width(term, 23)
        term.keys("3", "PageDown")
        frame = term.wait("Scrolled to another passage", lambda f: "Passage" in f and "Passage 000" not in f)
        anchor = re.search(r"Passage \d+", frame).group()
        change_width(term, directory, "contents_width", 23, 45)
        frame = at_width(term, 45)
        assert re.search(r"Passage \d+", frame).group() == anchor
        assert config(directory)["sidebar_width"] == 75
        size(term, 80)
        at_width(term, 39)
        size(term, 115)
        at_width(term, 45)
        term.keys("q")
        for _ in range(150):
            if term.run("has-session", "-t", "shelf", required=False).returncode:
                break
            time.sleep(0.02)
        else:
            raise AssertionError("Text reader stayed open")
    finally:
        term.run("kill-server", required=False)

    term = shelf.Terminal(directory, BINARY, args=[str(text)])
    try:
        frame = at_width(term, 45)
        assert re.search(r"Passage \d+", frame).group() == anchor
    finally:
        term.run("kill-server", required=False)

    pdf = directory / "harbour.pdf"
    pdf.write_bytes(sources.pdf(tones=True))
    term = shelf.Terminal(directory, BINARY, args=[str(pdf)])
    try:
        at_width(term, 45)
        term.see("1 / 3")
        term.keys("p")
        term.see("Go to page")
        term.keys("2", "Enter")
        term.see("2 / 3")
        change_width(term, directory, "contents_width", 45, 20)
        frame = at_width(term, 20)
        assert "2 / 3" in frame
        assert config(directory)["sidebar_width"] == 75
        term.keys("q")
        for _ in range(150):
            if term.run("has-session", "-t", "shelf", required=False).returncode:
                break
            time.sleep(0.02)
        else:
            raise AssertionError("Fixed-page reader stayed open")
    finally:
        term.run("kill-server", required=False)

    term = shelf.Terminal(directory, BINARY, args=[str(pdf)])
    try:
        assert "2 / 3" in term.wait("Restored fixed page and width", lambda f: "2 / 3" in f and
                                    any(line.find("↔") == 20 for line in f.splitlines()))
    finally:
        term.run("kill-server", required=False)

    term = shelf.Terminal(directory, BINARY, url)
    try:
        shelf.login(term, url)
        size(term, 160)
        at_width(term, 75)
        assert config(directory)["contents_width"] == 20
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-splitters-") as tmp, shelf.Fixture(False) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            exercise(Path(tmp), server)
        finally:
            server.shutdown()
            worker.join()
    print("Passed three live splitters, cancellation, compact clamping, independent widths and reader restart.")
