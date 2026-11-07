#!/usr/bin/env python3
"""Measure the rendered page, not only the zoom value reported by the footer."""
import importlib.util
import re
import shutil
import subprocess
import tempfile
import time
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
        frame = term.wait("Dialog over the rendered sheet",lambda f:"Go to page" in f and ":" * 40 in f and "Page 1" in f)
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
        term.keys("r", "v")
        term.see("90° · Inverted colors")
        term.keys("b")
        term.see("Page bookmarked")
        def checkpoint_saved():
            state = scroll.saved(root) or {}
            return (state.get("block") == 1 and state.get("marks") == [[1, 0]]
                    and state.get("scan_appearance") == {"quarter_turns": 1, "inverted": True})
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and not checkpoint_saved():
            time.sleep(.05)
        assert checkpoint_saved(), scroll.saved(root)
        term.run("kill-server", required=False)
        term = scroll.start(root, book)
        term.see("2 / 3")
        term.keys("R")
        term.see("0° · Inverted colors")
        term.keys("v")
        term.see("0° · Original colors")
        scroll.stop(term)
    finally:
        term.run("kill-server", required=False)
# Six very wide presentation slides fit on one screen when zoomed out.
with tempfile.TemporaryDirectory(prefix="botui-small-sheets-") as tmp:
    root=Path(tmp)
    slides=root/"slides.pdf"
    slides.write_bytes(scroll.s.pdf(True, 600, 95, (.2,.3,.42,.6,.72,.99)))
    term=scroll.start(root,slides)
    try:
        term.see("1 / 6")
        term.keys("3", *["-"]*7)
        term.wait("Sixth sheet is rendered without scrolling",lambda f:"25%" in f and "%" * 12 in f and ":"*12 in f)
        scroll.stop(term)
    finally:
        term.run("kill-server",required=False)
def check_search(root, book, page_count):
    term = scroll.start(root, book)
    try:
        term.keys("3")
        pixels = re.compile(r"([.:%#*=+@-])\1{14,}")
        paper = term.wait("Rendered first sheet", lambda f:
                          f"1 / {page_count}" in f and pixels.search(f))
        rows = [line for line in paper.splitlines() if pixels.search(line)]
        term.keys("/")
        overlay = term.wait("Search over visible paper and contents", lambda f:
                            "Find in text layer" in f and "Page 1" in f
                            and any(row in f for row in rows))
        assert "[ Search ↵ ]" in overlay and "[ Cancel ]" in overlay

        term.type("исправить")
        term.keys("C-u")
        paste = "\x1b[200~Lightouse atlas\x1b[201~"
        term.keys("-H", *[f"{b:02x}" for b in paste.encode()])
        term.keys("Home", *["Right"] * 5, "h", "Home", "Delete", "L", "End", "x", "BSpace")
        term.click("[ Search ↵ ]", term.see("Lighthouse atlas▏"))
        term.wait("Search reaches the matching sheet", lambda f:
                  f"2 / {page_count}" in f and "1 matching page" in f)

        term.keys("/")
        term.see("Lighthouse atlas▏")
        term.keys("C-u")
        term.type("discard this")
        term.click("[ Cancel ]", term.see("discard this▏"))
        term.keys("F5")
        term.see("Lighthouse atlas")
        term.keys("Escape")
        term.see(f"2 / {page_count}")
        scroll.stop(term)
    finally:
        term.run("kill-server", required=False)


with tempfile.TemporaryDirectory(prefix="botui-scan-search-") as tmp:
    root = Path(tmp)
    pdf = root / "search.pdf"
    pdf.write_bytes(scroll.s.pdf())
    check_search(root, pdf, 3)
    if all(shutil.which(tool) for tool in ("c44", "djvm", "djvused")):
        pages = []
        for i, color in enumerate((76, 204)):
            ppm = root / f"page{i}.ppm"
            ppm.write_bytes(b"P6\n120 175\n255\n" + bytes([color]) * (3 * 120 * 175))
            page = ppm.with_suffix(".djvu")
            subprocess.run(["c44", ppm, page], check=True)
            pages.append(page)
        djvu = root / "search.djvu"
        subprocess.run(["djvm", "-c", djvu, *pages], check=True)
        (root / "text").write_text('(page 0 0 120 175 (line 0 0 110 40 "Lighthouse atlas"))')
        subprocess.run(["djvused", "-s", "-e", "select 2; set-txt text", djvu], cwd=root, check=True)
        check_search(root, djvu, 2)

print("Passed physical zoom, page jump and editable text search over rendered sheets.")
