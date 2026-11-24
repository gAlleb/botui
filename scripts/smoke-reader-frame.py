#!/usr/bin/env python3
"""A reader frame may reflow pages, but must preserve source positions and selections."""
import argparse
import hashlib
import importlib.util
import json
import os
import platform
from pathlib import Path
import re
import shutil
import tempfile
import time
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()
PARAGRAPH = " ".join(f"word{i:03}" for i in range(900))
TARGET = "word173"
PROGRESS = re.compile(r"(\d+) / (\d+)  ·  \d+%")


def fixture(path):
    with zipfile.ZipFile(path, "w") as book:
        book.writestr("mimetype", "application/epub+zip")
        book.writestr("META-INF/container.xml", '''<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
          <rootfiles><rootfile full-path="book.opf"/></rootfiles></container>''')
        book.writestr("book.opf", '''<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Reader frame fixture</dc:title>
          <dc:identifier id="id">reader-frame-fixture</dc:identifier><dc:language>en</dc:language></metadata>
          <manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/>
          <item id="two" href="two.xhtml" media-type="application/xhtml+xml"/>
          <item id="plate" href="plate.png" media-type="image/png"/>
          <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest>
          <spine><itemref idref="one"/><itemref idref="two"/></spine></package>''')
        book.writestr("nav.xhtml", '''<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
          <body><nav epub:type="toc"><ol><li><a href="one.xhtml">Chapter 1. Departure</a></li>
          <li><a href="two.xhtml">Chapter 2. Return</a></li></ol></nav></body></html>''')
        book.writestr("one.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body>'
                      '<h1>Chapter 1. Departure</h1><p>' + PARAGRAPH + '</p><p>BeforePlate</p>'
                      '<img src="plate.png" style="width:120px;height:90px"/>'
                      '<p>AfterPlate</p></body></html>')
        book.writestr("two.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body>'
                      '<h1>Chapter 2. Return</h1>' + ''.join(
                          f'<p>Following passage {i:03} continues the second chapter.</p>'
                          for i in range(50)) + '</body></html>')
        book.writestr("plate.png", shelf.ORIGINAL_COVER)


def framed(frame):
    top = frame.partition("\n")[0]
    return top.startswith(("╭", "┌")) and top.rstrip().endswith(("╮", "┐"))


def page_body(frame):
    # A search hit and the first word on its ordinary page have different
    # progress percentages, even when the displayed book content is identical.
    rows = frame.rstrip("\n").splitlines()
    return rows[1:-1] if framed(frame) else rows


def target_visible(frame):
    return any(TARGET in row for row in frame.splitlines()[:-1])


def picture_visible(frame):
    # tmux exposes the monochrome pixel preview, not Kitty/Sixel graphics.
    # None of the fixture's prose contains a run of these image characters.
    return bool(re.search(r"[.:+*=#%@-]{4,}", frame))


def search(term, query):
    term.keys("Space", "f")
    term.see("Find:")
    term.keys("C-u")
    term.type(query)
    term.wait("Search results for the full query", lambda f:
              query in f and re.search(r"[1-9]\d* (?:match(?:es)?\b|· refine)", f))
    term.keys("Enter")
    return term.wait("Search reaches the passage", lambda f: "Find:" not in f and query in f)


def close(term):
    term.keys("Space", "q")
    deadline = time.monotonic() + 8
    while term.run("has-session", "-t", "shelf", required=False).returncode == 0:
        if time.monotonic() >= deadline:
            raise AssertionError("Reader did not close and save its position")
        time.sleep(.04)
    term.run("kill-server", required=False)


def isolated_reader(root, book):
    with book.open("rb") as handle:
        digest = hashlib.file_digest(handle, "sha256").hexdigest()
    record = root / "state/botui/positions" / (digest + ".json")
    helpers = root / "bin"
    helpers.mkdir()
    for name in ("wl-copy", "xclip", "xsel", "pbcopy"):
        helper = helpers / name
        helper.write_text('#!/bin/sh\ncat > "$HOME/copied"\n')
        helper.chmod(0o755)

    def start():
        return shelf.Terminal(root, Path("/usr/bin/env"), args=[
            "NO_COLOR=1", f"PATH={helpers}{os.pathsep}{os.environ['PATH']}", str(BINARY), str(book),
        ])

    def saved_position():
        data = json.loads(record.read_text())
        return data["block"], data["byte"], data.get("illustration")

    return start, saved_position


def check(root, mode):
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'zen = true\nzen_progress = false\nborders = "rounded"\n'
                      f'reading_layout = "{mode}"\npage_width = 160\nleader_delay_ms = 1\n')
    book = root / "frame.epub"
    fixture(book)
    start, saved_position = isolated_reader(root, book)

    def check_selection_row(term, before):
        book_rows = before.splitlines()[:-1]
        term.keys("Space", "c", "V")
        selected = term.see("VISUAL")
        assert selected.splitlines()[:-1] == book_rows, "Selecting moved the book"
        term.keys("Space", "a", "y")
        copied = term.see("Copied to clipboard")
        assert "Copied to clipboard" in copied.splitlines()[-1], copied
        assert copied.splitlines()[:-1] == book_rows, "Copying moved the book"
        assert (root / "copied").read_text() == PARAGRAPH
        restored = term.see("NORMAL")
        assert restored.splitlines()[:-1] == book_rows, "Dismissing feedback moved the book"
        term.keys("Escape")
        term.wait("Leaving Vim restores the original frame", lambda frame: frame == before)

    term = start()
    try:
        term.see("word000")
        search(term, TARGET)
        term.wait("Chapter page contains the exact interior passage", target_visible)
        close(term)
        original = saved_position()
        assert original[1] > 0, ("Fixture did not reach inside the long paragraph", original)

        term = start()
        clean = term.wait("Reopen clean Zen at the saved word", lambda f: not framed(f)
                          and target_visible(f))
        term.keys("Space", "p")
        term.wait("Frame and progress retain the source passage", lambda f: framed(f)
                  and "Reader frame fixture" in f.splitlines()[0] and PROGRESS.search(f) and target_visible(f))
        close(term)
        assert saved_position() == original, "Showing the frame changed the saved source address"

        term = start()
        bordered = term.wait("Reopen framed Zen at the same word", lambda f: framed(f)
                             and PROGRESS.search(f) and target_visible(f))
        check_selection_row(term, bordered)
        term.keys("Space", "p")
        term.wait("Removing the frame restores the clean layout", lambda f: f == clean)
        term.keys("Space", "z")
        normal = term.wait("Normal reader keeps the word inside its frame", lambda f: framed(f)
                           and "[Space z Zen]" in f and PROGRESS.search(f) and target_visible(f))
        border = normal.splitlines()[-2]
        assert PROGRESS.search(border), normal
        assert "Chapter 1" not in border, normal
        assert re.search(r"1/2 · Chapter 1\. Departure · \d+% ╮$", normal.splitlines()[0]), normal
        assert not PROGRESS.search(normal.splitlines()[-1]), normal
        assert normal.splitlines()[0].count("Reader frame fixture") == 1, normal
        assert "Reader frame fixture" not in normal.splitlines()[-1], normal
        check_selection_row(term, normal)
        term.click("[Space z Zen]", normal)
        term.wait("Return from normal reader restores clean Zen", lambda f: f == clean)
        check_selection_row(term, clean)
        term.keys("Space", "Z")
        full = term.wait("Book uses the last row of full-height Zen",
                         lambda f: not framed(f) and 'word' in f.splitlines()[-1])
        check_selection_row(term, full)
        close(term)
        term = start()
        term.wait("Reopen in full-height Zen", lambda f: f == full)
        term.keys("Space", "Z")
        term.wait("Switch back to ordinary Zen", lambda f: f == clean)

        # V selects the source paragraph, including text outside this viewport.
        # Changing the frame must not copy padding, omit text or change the range.
        term.keys("Space", "c", "V")
        term.see("VISUAL")
        for visible in (True, False):
            term.keys("Space", "p")
            term.wait("Selected paragraph survives frame reflow", lambda f: framed(f) == visible
                      and "VISUAL" in f and target_visible(f))
            copied = root / "copied"
            copied.unlink(missing_ok=True)
            term.keys("Space", "a", "y")
            term.wait("Copy retains the complete source paragraph", lambda _: copied.exists()
                      and copied.read_text() == PARAGRAPH)
            if visible:
                term.keys("V")
                term.see("VISUAL")
        term.keys("Escape")
        term.wait("Leave Vim", lambda f: "NORMAL" not in f and "VISUAL" not in f)
        close(term)
        assert saved_position() == original, "Frame/Zen toggles changed the saved source address"

        term = start()
        term.wait("Reopen after all toggles", lambda f: not framed(f) and target_visible(f))
        for visible in (False, True):
            if visible:
                term.keys("Space", "p")
            search(term, "AfterPlate")
            plate = term.wait("Illustration is present beside its source text", lambda f:
                              framed(f) == visible and "AfterPlate" in f and picture_visible(f)
                              and (not visible or PROGRESS.search(f)))
            term.keys("Left")
            term.wait("Previous page is different", lambda f: f != plate and "word" in f)
            term.keys("Right")
            term.wait("Back/forward restores the same illustration page",
                      lambda f: page_body(f) == page_body(plate))
        close(term)
    finally:
        term.run("kill-server", required=False)


def file_digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


class Metrics:
    def __init__(self, book, images):
        self.report = {
            "host": platform.node(), "kernel": platform.release(), "arch": platform.machine(),
            "binary_sha256": file_digest(BINARY), "book_sha256": file_digest(book),
            "inline_images": images, "geometry": "115x30 -> 79x23", "steps": {},
        }

    def record(self, name, started, term):
        elapsed = round((time.monotonic() - started) * 1000, 1)
        pane = term.run("display-message", "-p", "-t", "shelf", "#{pane_pid}").stdout.strip()
        # tmux may run the reader through a shell. Only sample the exact executable.
        pids = [int(pane)]
        while pids:
            pid = pids.pop()
            proc = Path("/proc") / str(pid)
            try:
                if (proc / "exe").samefile(BINARY):
                    status = (proc / "status").read_text()
                    rss = int(re.search(r"^VmRSS:\s+(\d+)", status, re.M)[1])
                    hwm = int(re.search(r"^VmHWM:\s+(\d+)", status, re.M)[1])
                    self.report["steps"][name] = {"ms": elapsed, "rss_kib": rss, "peak_rss_kib": hwm}
                    return
                pids.extend(map(int, (proc / "task" / str(pid) / "children").read_text().split()))
            except (FileNotFoundError, ProcessLookupError):
                pass
        raise RuntimeError("Cannot locate the running reader under /proc for its memory sample")


def check_real_book(root, source, word, metrics=None, images=True):
    book = root / ("edition" + source.suffix)
    shutil.copyfile(source, book)
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('zen=true\nzen_progress=true\nreading_layout="spread"\npage_width=42\n'
                      + ('hide_inline_images=false\n' if images else 'hide_inline_images=true\n'))
    start, position = isolated_reader(root, book)

    def reading(frame):
        return PROGRESS.search(frame) and "Find:" not in frame

    def matched(frame):
        return reading(frame) and word.casefold() in frame.casefold()

    def copy_paragraph(term):
        clipboard = root / "copied"
        clipboard.unlink(missing_ok=True)
        term.keys("Space", "a", "y")
        term.wait("Selected paragraph reached the private clipboard",
                  lambda _: clipboard.exists() and word.casefold() in clipboard.read_text().casefold())
        return clipboard.read_text()

    def measure(name, started):
        if metrics:
            metrics.record(name, started, term)

    started = time.monotonic()
    term = start()
    try:
        term.wait("Open the external edition", reading, timeout=60)
        measure("first_open", started)
        term.keys("Space", "f"); term.see("Find:")
        started = time.monotonic()
        term.type(word)
        term.wait("Find the control word in the book",
                  lambda f: re.search(r"▏\s+[1-9]\d* (?:matches|· refine)", f), timeout=30)
        measure("word_results", started)
        started = time.monotonic()
        term.keys("Enter")
        term.wait("Open the selected occurrence", matched)
        measure("open_result", started)
        close(term)
        source_address = position()

        started = time.monotonic()
        term = start()
        first = term.wait("Restore the search position", matched, timeout=60)
        measure("resume", started)
        pages = PROGRESS.search(first)
        page, total = map(int, pages.groups())
        assert total > 1, "Use a book that fills more than one spread"
        forward, back = ("Right", "Left") if page < total else ("Left", "Right")
        started = time.monotonic()
        term.keys(forward)
        term.wait("Move to a different spread", lambda f: reading(f) and pages.group() not in f)
        measure("turn_" + forward.lower(), started)
        started = time.monotonic()
        term.keys(back)
        term.wait("Recover the spread after a round trip", lambda f: page_body(f) == page_body(first))
        measure("turn_" + back.lower(), started)

        # A return turn focuses the visible page opening. Select the searched
        # paragraph again before testing that its source range survives a resize.
        search(term, word)
        term.keys("Space", "c", "V"); term.see("VISUAL")
        before = copy_paragraph(term)
        term.keys("V"); term.see("VISUAL")
        started = time.monotonic()
        term.run("resize-window", "-t", "shelf", "-x", "79", "-y", "23")
        term.wait("Resize while the selection is active",
                  # Zen's selection commands temporarily cover the bottom
                  # progress row without changing the book's viewport.
                  lambda f: len(f.splitlines()) == 23 and "VISUAL" in f
                  and word.casefold() in f.casefold() and framed(f))
        measure("resize_narrow", started)
        assert copy_paragraph(term) == before, "Resize changed the selected source paragraph"
        term.keys("Escape")
        term.wait("Return to reading", lambda f: matched(f) and "NORMAL" not in f and "VISUAL" not in f)
        started = time.monotonic()
        term.run("resize-window", "-t", "shelf", "-x", "115", "-y", "30")
        term.wait("Restore the original size", lambda f: len(f.splitlines()) == 30 and matched(f))
        measure("resize_wide", started)
        close(term)
        assert position() == source_address, "Reading position changed during turn, selection or resize"

        term = start()
        term.wait("Reopen on the original word", matched, timeout=60)
        close(term)
        assert position() == source_address, "Reopening changed the saved source address"
        if metrics:
            disk_cache = root / "cache/botui"
            metrics.report["disk_cache_bytes"] = sum(p.stat().st_size for p in disk_cache.rglob("*") if p.is_file())
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--book", type=Path, help="exercise a copy of this EPUB or FB2")
    parser.add_argument("--word", help="visible word in a prose paragraph of the book")
    parser.add_argument("--metrics", action="store_true", help="report Linux terminal timings, botui RSS and cache bytes as JSON")
    parser.add_argument("--images", choices=["on", "off", "both"], help="inline illustrations for metrics (default: both)")
    args = parser.parse_args()
    if args.images and not args.metrics:
        parser.error("--images requires --metrics")
    if args.metrics and (not args.book or platform.system() != "Linux"):
        parser.error("--metrics requires --book on Linux")
    if args.book or args.word is not None:
        if not args.book or args.book.suffix.lower() not in (".epub", ".fb2") or not args.book.is_file():
            parser.error("--book requires a readable EPUB or FB2 file")
        if not args.word or len(args.word.split()) != 1:
            parser.error("--word requires one word from the book")
        modes = [True, False] if args.metrics and args.images in (None, "both") else [args.images != "off"]
        for images in modes:
            metrics = Metrics(args.book, images) if args.metrics else None
            with tempfile.TemporaryDirectory(prefix="botui-real-reading-") as tmp:
                check_real_book(Path(tmp), args.book, args.word.strip(), metrics, images)
            if metrics:
                print(json.dumps(metrics.report, ensure_ascii=False), flush=True)
        print(f"PASS {args.book.name}: search, page round trip, selection, resize and reopen")
    else:
        for mode in ("single", "spread"):
            with tempfile.TemporaryDirectory(prefix=f"botui-reader-frame-{mode}-") as tmp:
                check(Path(tmp), mode)
            print(f"PASS {mode}: frame reflow, saved source address, Vim paragraph and illustration round trip", flush=True)
