#!/usr/bin/env python3
"""Follow successive in-book results in the actual reader, including the highlighted term."""
from pathlib import Path
import importlib.util
import io
import re
import tempfile
import time
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

with tempfile.TemporaryDirectory() as name:
    tmp = Path(name)
    path = tmp / "search.fb2"
    entries = [f"<p>Paragraph {i:03}</p>" for i in range(100)]
    for i in (12, 55, 88):
        entries[i] = f"<p>At lighthouse {i:03} we stopped.</p>"
    path.write_text("<FictionBook><body><section>" + "".join(entries) + "</section></body></FictionBook>")
    t = shelf.Terminal(tmp, shelf.ROOT / "target/debug/botui", args=[str(path)])
    try:
        t.see("Paragraph 000")
        t.keys('Space', 'l', '1', 'Space', 'f')
        t.see('[Tab: Exact phrase]')
        t.type('lighthouse bravo')
        t.see('No matches.')
        t.keys('Tab')
        t.see('3 matches')
        t.keys('Enter')
        t.keys('F5'); t.see('[Tab: Any word]')
        t.click('[Tab: Any word]', t.see('[Tab: Any word]'))
        t.see('No matches.')
        t.keys('Escape')
        t.wait('Results closed', lambda f: 'Find:' not in f)
        t.keys('Space', 'f'); t.see('[Tab: Exact phrase]')
        t.see("Find:")
        t.type("lighthouse")
        t.see("3 matches")
        t.keys("Enter")
        def highlighted(number):
            t.wait(f"selected occurrence {number}", lambda s: "Find:" not in s and f"lighthouse {number}" in s)
            ansi = t.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
            assert re.search(r"\x1b\[[0-9;]*7m" + "lighthouse" + r"\x1b\[[0-9;]*m " + number, ansi), repr(ansi)
        highlighted("012")
        t.keys("F3")
        highlighted("055")
        t.keys("F4")
        highlighted("012")
        t.keys("F4")
        highlighted("088")
        t.keys("F3")
        highlighted("012")
        t.keys("F5")
        t.see("3 matches")
        t.see("Find: lighthouse")
        t.keys("Down", "Enter")
        highlighted("055")

        def start_column(frame):
            return next(row.index("At lighthouse 055") for row in frame.splitlines() if "At lighthouse 055" in row)
        col = start_column(t.see("At lighthouse 055"))
        t.keys("F2"); t.see("Interface theme"); t.keys("Tab")
        t.click("First-line indent", t.see("First-line indent"))
        t.see("0–8 cells"); t.keys("C-u"); t.type("4"); t.keys("Enter")
        t.see("4 cells")
        t.click("Paragraph spacing", t.see("Paragraph spacing"))
        t.click("2 blank lines", t.see("2 blank lines"))
        t.see("First-line indent"); t.keys("Escape")
        t.wait("indent changes on the open book", lambda f: "At lighthouse 055" in f and start_column(f) == col + 4)
        highlighted("055")
        def two_blank_lines(frame, first, second):
            rows = frame.splitlines()
            return any(first in a and second in b for a, b in zip(rows, rows[3:]))
        t.wait("spacing applies to the open FB2", lambda f: two_blank_lines(f, "At lighthouse 055", "Paragraph 056"))
        t.keys('Space', 'b', 'a', 'F3')
        highlighted("088")
        t.keys('Space', 'b', 'a', 'Space', 'b', 'l')
        frame = t.see("Bookmarks: 2")
        assert frame.count("% · Local ·") == 2, frame
        t.keys("Home", "DC"); t.see("Delete bookmark?")
        t.keys("Escape"); t.see("Bookmarks: 2")
        t.click("[×]", t.see("[×]")); t.click("[ Delete bookmark ]", t.see("Delete bookmark?"))
        remaining = t.see("Bookmarks: 1")
        assert "088" in remaining and "055" not in remaining, remaining
        t.keys("Escape")
        t.see("At lighthouse 088")
        t.keys('Space', 'q')
        deadline = time.monotonic() + 5
        while t.run("has-session", "-t", "shelf", required=False).returncode == 0:
            if time.monotonic() >= deadline:
                raise AssertionError("Reader did not exit after saving the position")
            time.sleep(.04)
        t = shelf.Terminal(tmp, shelf.ROOT / "target/debug/botui", args=[str(path)])
        t.wait("saved spacing on reopen", lambda f: two_blank_lines(f, "lighthouse 088", "Paragraph 089"))
        t.keys('Space', 'b', 'l')
        assert "088" in t.see("Bookmarks: 1")
        t.keys('Escape', 'Space', 'q')
    finally:
        t.run("kill-server", required=False)
print("Passed search navigation, highlighting, live paragraph indent and spacing, and bookmark deletion.")

with tempfile.TemporaryDirectory() as name:
    tmp = Path(name)
    path = tmp / "responsive.epub"
    chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head>
      <style>@media (max-width: 400px) { .wide { display: none } }</style></head>
      <body><p>Departure</p><p class="wide">Wide lighthouse</p><p>Coastal lighthouse</p></body></html>"""
    with zipfile.ZipFile(io.BytesIO(shelf.epub())) as template, zipfile.ZipFile(path, "w") as book:
        for member in template.namelist():
            book.writestr(member, chapter if member == "c.xhtml" else template.read(member))
    t = shelf.Terminal(tmp, shelf.ROOT / "target/debug/botui", args=[str(path)])
    try:
        def selected(label):
            def has_highlight(frame):
                if "Find:" in frame or label + " lighthouse" not in frame:
                    return False
                ansi = t.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
                return re.search(label + r" \x1b\[[0-9;]*7mlighthouse\x1b", ansi)
            t.wait("highlight on " + label, has_highlight)

        def width(columns):
            t.run("resize-window", "-t", "shelf", "-x", str(columns), "-y", "30")
            t.wait("responsive chapter after resize", lambda f:
                   "Coastal lighthouse" in f and ("Wide lighthouse" in f) == (columns == 115))

        t.see("Departure"); t.keys("Space", "l", "1"); t.see("Wide lighthouse")
        t.keys("Space", "f"); t.see("Find:")
        t.type("lighthouse"); t.see("2 matches"); t.keys("Enter")
        selected("Wide")
        width(44)
        selected("Coastal")
        t.keys("F5"); t.see("1 match"); t.keys("Escape")
        t.keys("Escape")  # Hide the highlight, leaving index refresh to the next search action.
        width(115)
        t.keys("F5"); t.see("2 matches"); t.keys("Home", "Enter")
        selected("Wide")
        t.keys("Escape")
        width(44)
        for _ in range(2):
            t.keys("F3")
            selected("Coastal")
    finally:
        t.run("kill-server", required=False)
print("Passed responsive EPUB search after resizing, including highlight, F3 and F5.")
