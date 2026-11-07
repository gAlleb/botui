#!/usr/bin/env python3
"""Follow successive in-book results in the actual reader, including the highlighted term."""
from pathlib import Path
import importlib.util
import re
import tempfile

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
        t.keys("1", "/")
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
        t.see("4 cells"); t.keys("Escape")
        t.wait("indent changes on the open book", lambda f: "At lighthouse 055" in f and start_column(f) == col + 4)
        highlighted("055")
        t.keys("b", "F3")
        highlighted("088")
        t.keys("b", "g")
        t.see("Bookmarks: 2"); t.keys("Home", "DC"); t.see("Delete bookmark?")
        t.keys("Escape"); t.see("Bookmarks: 2")
        t.click("[×]", t.see("[×]")); t.click("[ Delete bookmark ]", t.see("Delete bookmark?"))
        remaining = t.see("Bookmarks: 1")
        assert "088" in remaining and "055" not in remaining, remaining
        t.keys("Escape", "q")
        t.run("kill-server", required=False)
        t = shelf.Terminal(tmp, shelf.ROOT / "target/debug/botui", args=[str(path)])
        t.see("lighthouse 088"); t.keys("g")
        assert "088" in t.see("Bookmarks: 1")
        t.keys("Escape", "q")
    finally:
        t.run("kill-server", required=False)
print("Passed search navigation, highlighted text, live paragraph indentation and per-row bookmark deletion.")
