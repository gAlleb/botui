#!/usr/bin/env python3
"""A reader frame may reflow pages, but must preserve source positions and selections."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import tempfile
import time
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()
PARAGRAPH = " ".join(f"word{i:03}" for i in range(900))
TARGET = "word173"
PROGRESS = re.compile(r"\d+ / \d+  ·  \d+%")


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
          <body><nav epub:type="toc"><ol><li><a href="one.xhtml">Frame chapter one</a></li>
          <li><a href="two.xhtml">Frame chapter two</a></li></ol></nav></body></html>''')
        book.writestr("one.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body>'
                      '<h1>Frame chapter one</h1><p>' + PARAGRAPH + '</p><p>BeforePlate</p>'
                      '<img src="plate.png" style="width:120px;height:90px"/>'
                      '<p>AfterPlate</p></body></html>')
        book.writestr("two.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body>'
                      '<h1>Frame chapter two</h1>' + ''.join(
                          f'<p>Following passage {i:03} continues the second chapter.</p>'
                          for i in range(50)) + '</body></html>')
        book.writestr("plate.png", shelf.ORIGINAL_COVER)


def framed(frame):
    return bool(re.search(r"[╭┌]─+[╮┐]", frame))


def target_on_first_line(frame):
    return TARGET in next((row for row in frame.splitlines() if re.search(r"word\d{3}", row)), "")


def picture_visible(frame):
    # tmux exposes the monochrome pixel preview, not Kitty/Sixel graphics.
    # None of the fixture's prose contains a run of these image characters.
    return bool(re.search(r"[.:+*=#%@-]{4,}", frame))


def search(term, query):
    term.keys("Space", "f")
    term.see("Find:")
    term.keys("C-u")
    term.type(query)
    term.see("1 match")
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


def check(root, mode):
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'zen = true\nzen_progress = false\nborders = "rounded"\n'
                      f'reading_layout = "{mode}"\npage_width = 160\nleader_delay_ms = 1\n')
    book = root / "frame.epub"
    fixture(book)
    record = root / "state/botui/positions" / (hashlib.sha256(book.read_bytes()).hexdigest() + ".json")
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

    term = start()
    try:
        term.see("word000")
        search(term, TARGET)
        term.wait("Exact interior passage is at the top", target_on_first_line)
        close(term)
        original = saved_position()
        assert original[1] > 0, ("Fixture did not reach inside the long paragraph", original)

        term = start()
        clean = term.wait("Reopen clean Zen at the saved word", lambda f: not framed(f)
                          and target_on_first_line(f))
        term.keys("Space", "p")
        term.wait("Frame and progress retain the source passage", lambda f: framed(f)
                  and PROGRESS.search(f) and target_on_first_line(f))
        close(term)
        assert saved_position() == original, "Showing the frame changed the saved source address"

        term = start()
        term.wait("Reopen framed Zen at the same word", lambda f: framed(f)
                  and PROGRESS.search(f) and target_on_first_line(f))
        term.keys("Space", "p")
        term.wait("Removing the frame restores the clean layout", lambda f: f == clean)
        term.keys("Space", "z")
        term.wait("Normal reader keeps the word inside its frame", lambda f: framed(f)
                  and "Contents" in f and target_on_first_line(f))
        term.keys("Space", "z")
        term.wait("Return from normal reader restores clean Zen", lambda f: f == clean)

        # V selects the source paragraph, including text outside this viewport.
        # Changing the frame must not copy padding, omit text or change the range.
        term.keys("Space", "c", "V")
        term.see("VISUAL")
        for visible in (True, False):
            term.keys("Space", "p")
            term.wait("Selected paragraph survives frame reflow", lambda f: framed(f) == visible
                      and "VISUAL" in f and target_on_first_line(f))
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
        term.wait("Reopen after all toggles", lambda f: not framed(f) and target_on_first_line(f))
        search(term, "BeforePlate")
        for visible in (False, True):
            if visible:
                term.keys("Space", "p")
            plate = term.wait("Illustration is present beside its source text", lambda f:
                              framed(f) == visible and "BeforePlate" in f and picture_visible(f)
                              and (not visible or PROGRESS.search(f)))
            term.keys("Left")
            term.wait("Previous page is different", lambda f: f != plate and "word" in f)
            term.keys("Right")
            term.wait("Back/forward restores the same illustration page", lambda f: f == plate)
        close(term)
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    for mode in ("single", "spread"):
        with tempfile.TemporaryDirectory(prefix=f"botui-reader-frame-{mode}-") as tmp:
            check(Path(tmp), mode)
        print(f"PASS {mode}: frame reflow, saved source address, Vim paragraph and illustration round trip", flush=True)
