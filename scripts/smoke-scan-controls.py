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

def apply_colors(term, label):
    term.keys("Space", "t")
    term.click(label, term.see("Page colors · [ / ]"))
    term.see(f"● {label}")
    term.keys("Enter")


with tempfile.TemporaryDirectory(prefix="botui-width-") as tmp:
    root = Path(tmp)
    book = root / "sheets.pdf"
    book.write_bytes(scroll.s.pdf(True))
    term = scroll.start(root, book)
    try:
        term.see("1 / 3")
        term.click("[Space m Menu]", term.see("[Space m Menu]"))
        frame = term.see("Previous page")
        width, height = map(int, term.run("display-message", "-p", "-t", "shelf",
                                          "#{pane_width} #{pane_height}").stdout.split())
        row, line = next((y, line) for y, line in enumerate(frame.splitlines()) if "Resume" in line)
        assert (line.index("Resume") - 1, row - 1) == ((width - 48) // 2, (height - 20) // 2)
        term.click("Resume", frame)
        term.click("[Space z Zen]", term.see("[Space z Zen]"))
        term.keys("Space", "p")
        term.see("1 / 3  ·  33%")
        term.keys("Right")
        term.see("2 / 3  ·  67%")
        term.keys("Left", "Space", "p", "Space", "z")
        term.see("Space l w")
        term.keys("1", "2", "3")
        frame = term.wait("Paper fits pane", lambda f: "Fit width" in f and ":" * 60 in f)
        left, right = paper(frame)
        term.keys("Space", "Z")
        term.wait("PDF paints the bottom row in full-height Zen",
                  lambda f: ":" * 40 in f.splitlines()[-1])
        term.keys("Space", "p")
        term.wait("Switch directly from full-height to framed Zen",
                  lambda f: "╰" in f.splitlines()[-1])
        term.keys("Space", "z")
        term.see("[Space l w 100%]")
        term.keys("-")
        frame = term.wait("Paper shrinks", lambda f: "90%" in f and ":" * 35 in f and paper(f)[0] > left)
        a, b = paper(frame)
        assert abs((right - left) * .9 - (b - a)) <= 2 and abs((a-left)-(right-b)) <= 1
        term.keys("+")
        term.wait("Direct plus restores width", lambda f: "100%" in f and paper(f)[1] - paper(f)[0] > b - a)
        term.keys("-"); term.see("90%")
        term.keys("="); term.see("100%")
        term.keys("-"); frame = term.see("90%")
        term.click("[Space l w 90%]", frame)
        term.click("[ − ]", term.see("Page width / zoom"))
        term.see("80%")
        term.keys("[", "[")
        term.see("Scroll gap: 0 rows")
        term.click("[ ] ]", term.see("Scroll gap: 0 rows"))
        term.click("[ [ ]", term.see("Scroll gap: 1 rows"))
        term.see("Scroll gap: 0 rows")
        term.keys("]")
        term.see("Scroll gap: 1 rows")
        term.keys("[")
        term.see("Scroll gap: 0 rows")
        term.click("[ Fit width ]", term.see("[ Fit width ]"))
        term.see("100%")
        term.click("[ Done ]", term.see("[ Done ]"))
        term.keys("Space", "r", "p")
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
        term.see("[Space l w 90%]")
        term.keys("Space", "r", "0")
        term.see("[Space l w 100%]")
        term.keys("Space", "r", "r")
        apply_colors(term, "Invert original")
        term.see("Page colors · Invert original")
        term.keys("Space", "b", "a")
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
        term.keys("Space", "r", "R")
        term.see("0° · Inverted colors")
        apply_colors(term, "Use reading theme")
        term.see("Page colors · Use reading theme")
        # Cursor feedback occupies the status line; verify the persisted color
        # state as well as the ordinary reading notifications above and below.
        term.keys("Space", "c")
        for label, inverted, themed in [("As in document", False, False), ("Invert original", True, False), ("Use reading theme", False, True)]:
            apply_colors(term, label)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                appearance = (scroll.saved(root) or {}).get("scan_appearance", {})
                if (appearance.get("inverted", False) == inverted
                        and appearance.get("terminal_paper", False) == themed):
                    break
                time.sleep(.05)
            else:
                raise AssertionError((inverted, themed, appearance))
        term.keys("Space", "c")
        apply_colors(term, "As in document")
        term.see("Page colors · As in document")
        scroll.stop(term)
    finally:
        term.run("kill-server", required=False)
# Six very wide presentation slides fit on one screen when zoomed out.
with tempfile.TemporaryDirectory(prefix="botui-small-sheets-") as tmp:
    root=Path(tmp)
    slides=root/"slides.pdf"
    slides.write_bytes(scroll.s.pdf(True, 600, 95, (.8,.3,.42,.6,.72,.99)))
    term=scroll.start(root,slides)
    try:
        term.see("1 / 6")
        term.keys("Space", "l", "3", *[key for _ in range(7) for key in ["Space", "r", "-"]])
        term.wait("Sixth sheet is rendered without scrolling",lambda f:"25%" in f and "%" * 12 in f and ":"*12 in f)
        # At zero gap the next sheet touches the previous one. Only the
        # margins show the divider, on the first row of the following sheet.
        def flush_seam(frame):
            lines = frame.splitlines()
            return any("#" * 12 in above and re.search(r"·{5,}:{12,}·{5,}", below)
                       for above, below in zip(lines, lines[1:]))
        term.wait("Flush sheets with a divider in both margins", flush_seam)
        term.keys("Space", "l", "w", *(["]"] * 9), "Enter")
        for zoom in (25, 50):
            if zoom == 50:
                term.keys("+", "+")
            frame = term.wait(f"Nine-row gap at {zoom}%", lambda f:
                              f"{zoom}%" in f and "·" * 12 in f and ":" * 12 in f and "Done" not in f)
            lines = frame.splitlines()
            last = max(i for i, line in enumerate(lines) if "#" * 12 in line)
            seam = next(i for i, line in enumerate(lines) if "·" * 12 in line)
            first = next(i for i, line in enumerate(lines) if ":" * 12 in line)
            assert (seam - last - 1, first - seam - 1) == (4, 4), frame
        # Start the viewport inside the gap, exactly on the separator.
        top = next(i for i, line in enumerate(lines) if "#" * 12 in line)
        # Reverse direction within one burst. The net travel must still put
        # the divider at precisely the same row as individual Down keys.
        down = "\x1b[<65;70;8M"
        up = "\x1b[<64;70;8M"
        term.type(down * (seam - top + 2) + up * 2)
        term.wait("Centered divider at top of viewport",
                  lambda f: "·" * 12 in f.splitlines()[top] and ":" * 12 in f.splitlines()[top + 5])
        term.keys("Down")
        term.wait("Divider scrolls off the top",
                  lambda f: "·" * 12 not in f.splitlines()[top] and ":" * 12 in f.splitlines()[top + 4])
        scroll.stop(term)
    finally:
        term.run("kill-server",required=False)
def check_search(root, book, page_count):
    term = scroll.start(root, book)
    try:
        term.keys("1", "2", "3")
        pixels = re.compile(r"([.:%#*=+@-])\1{14,}")
        paper = term.wait("Rendered first sheet", lambda f:
                          f"1 / {page_count}" in f and pixels.search(f))
        rows = [line for line in paper.splitlines() if pixels.search(line)]
        term.keys("Space", "f")
        overlay = term.wait("Search over visible paper and contents", lambda f:
                            "Find in text layer" in f and "Page 1" in f
                            and any(row in f for row in rows))
        assert "[ Search ↵ ]" in overlay and "[ Cancel ]" in overlay

        term.type("исправить")
        term.keys("C-u")
        paste = "\x1b[200~Lightouse atlas\x1b[201~"
        term.keys("-H", *[f"{b:02x}" for b in paste.encode()])
        term.keys("Home", *["Right"] * 5, "h", "Home", "Delete", "L", "End", "x", "BSpace")
        term.click("[ Search ↵ ]", term.see("Lighthouse atlas"))
        term.wait("Search reaches the matching sheet", lambda f:
                  f"2 / {page_count}" in f and "Match 1 / 1" in f)

        term.keys("Space", "f")
        term.see("Lighthouse atlas")
        term.keys("C-u")
        term.type("discard this")
        term.click("[ Cancel ]", term.see("discard this"))
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


def check_frame_position(root, mode):
    """A changed viewport must not reinterpret the saved page or scroll fraction."""
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'zen = true\nzen_progress = false\nborders = "rounded"\n'
                      f'reading_layout = "{mode}"\nleader_delay_ms = 1\n')
    book = root / "frame.pdf"
    book.write_bytes(scroll.s.pdf(True, shades=(.3, .5, .7, .8)))
    pixels = re.compile(r"([.:%#*=+@-])\1{14,}")

    def has_frame(frame):
        return bool(re.fullmatch(rf"[╭┌] {re.escape(book.stem)} ─+[╮┐]", frame.partition("\n")[0]))

    def position():
        state = scroll.saved(root) or {}
        return tuple(state.get(k, 0) for k in ("block", "scan_offset", "spread", "scrolling"))

    term = scroll.start(root, book)
    try:
        term.wait("Clean Zen PDF is rendered", lambda f: pixels.search(f) and not has_frame(f))
        term.keys("Space", "r", "p")
        term.see("Go to page")
        term.keys("C-u", "2", "Enter")
        term.wait("Second PDF page is saved", lambda f: "Go to page" not in f
                  and pixels.search(f) and position()[0] == 1)
        if mode == "scroll":
            term.keys("Down", "Down", "Down")
            term.wait("Nonzero position within the physical page", lambda _: position()[1] > 0)
        scroll.stop(term)
        original = position()
        assert original[0] == 1 and bool(original[2]) == (mode == "spread"), original
        assert bool(original[3]) == (mode == "scroll"), original
        term = scroll.start(root, book)
        term.wait("PDF reopens in clean Zen", lambda f: pixels.search(f) and not has_frame(f))
        term.keys("Space", "z")
        normal = term.wait("PDF progress is in the normal reading frame", lambda f:
                           "[Space z Zen]" in f and re.search(r"2 / 4  ·  \d+%", f.splitlines()[-2]))
        assert "2 / 4" not in normal.splitlines()[-1], normal
        assert book.stem in normal.splitlines()[0], normal
        term.keys("Space", "z")
        term.wait("Return to clean Zen", lambda f: pixels.search(f) and "[Space z Zen]" not in f)
        scroll.stop(term)
        assert position() == original, (mode, original, position())
        for visible in (True, False):
            term = scroll.start(root, book)
            term.wait("PDF reopens with the saved frame setting", lambda f:
                      pixels.search(f) and has_frame(f) != visible)
            term.keys("Space", "p")
            term.wait("Frame toggles around the rendered page", lambda f:
                      pixels.search(f) and has_frame(f) == visible
                      and (not visible or "2 / 4" in f))
            scroll.stop(term)
            assert position() == original, (mode, visible, original, position())
    finally:
        term.run("kill-server", required=False)


for mode in ("single", "spread", "scroll"):
    with tempfile.TemporaryDirectory(prefix=f"botui-pdf-frame-{mode}-") as tmp:
        check_frame_position(Path(tmp), mode)

print("Passed physical zoom, page jump and editable text search over rendered sheets.")
