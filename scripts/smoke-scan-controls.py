#!/usr/bin/env python3
"""Measure the rendered page, not only the zoom value reported by the footer."""
import importlib.util
import re
import tempfile
from pathlib import Path

here = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scroll", here / "smoke-scroll.py")
scroll = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scroll)

def paper(frame):
    spans = [m.span() for line in frame.splitlines() for m in re.finditer(r":{25,}", line)]
    return max(spans, key=lambda p: p[1] - p[0])

with tempfile.TemporaryDirectory(prefix="botui-width-") as tmp:
    root = Path(tmp)
    book = root / "sheets.pdf"
    book.write_bytes(scroll.s.pdf(True))
    term = scroll.start(root, book)
    try:
        term.see("1 / 3")
        term.keys("3")
        frame = term.wait("Paper fits pane", lambda f: "Fit width" in f and ":" * 60 in f)
        left, right = paper(frame)
        term.keys("-")
        frame = term.wait("Paper shrinks", lambda f: "90%" in f and ":" * 35 in f and paper(f)[0] > left)
        a, b = paper(frame)
        assert abs((right - left) * .9 - (b - a)) <= 2 and abs((a-left)-(right-b)) <= 1
        term.click("[W 90%]", frame)
        term.click("[ − ]", term.see("Page width / zoom"))
        term.see("80%")
        term.click("[ Fit width ]", term.see("[ Fit width ]"))
        term.see("100%")
        term.click("[ Done ]", term.see("[ Done ]"))
        term.keys("p")
        frame = term.see("Go to page")
        assert ":" * 40 in frame and "Page 1" in frame
        line = next(line for line in frame.splitlines() if "Go to page" in line)
        x = line.index("Go to page") - 2
        assert abs((x + 17) - (left + right)/2) <= 2, (x,left,right)
        term.keys("9", "Enter")
        term.see("Enter 1–3")
        term.keys("C-u", "2", "Enter")
        term.see("2 / 3")
        term.keys("-")
        term.see("90%")
        scroll.stop(term)
        term = scroll.start(root, book)
        term.see("[W 90%]")
        term.keys("0")
        term.see("[W 100%]")
        scroll.stop(term)
    finally:
        term.run("kill-server", required=False)
print("Passed: physical PDF width, symmetric margins, mouse controls, zoom persistence and page dialog over the sheet.")
