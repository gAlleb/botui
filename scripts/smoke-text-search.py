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
        t.keys("q")
    finally:
        t.run("kill-server", required=False)
print("Passed in-book next/previous search, wrapping and visible result highlighting.")
