#!/usr/bin/env python3
"""Check edition-specific overrides, inheritance, quick controls and the global TOML."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import tomllib

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
binary = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()


def open_settings(t):
    t.keys("F2")
    t.see("Appearance")
    t.keys("Right")
    t.click("[S This book]", t.see("[S This book]"))
    return t.see("Use global settings for this book")


def close_settings(t):
    t.keys("Escape")
    t.see("[S This book]")
    t.keys("Escape")


def line_gap(t, expected):
    def matches(frame):
        lines = frame.splitlines()
        first = next((i for i, s in enumerate(lines) if "First paragraph" in s), -1)
        second = next((i for i, s in enumerate(lines) if "Second paragraph" in s), -1)
        return first >= 0 and second == first + 1 + expected
    return t.wait(f"Actual space between paragraphs is {expected}", matches)


def run(tmp):
    cfg = tmp / "config/botui/config.toml"
    cfg.parent.mkdir(parents=True)
    cfg.write_text('zen = false\nreading_layout = "single"\npage_width = 62\nparagraph_spacing = 1\n'
                   'paragraph_indent = 0\nscan_theme = "terminal"\nscan_page_colors = "original"\n')
    text = tmp / "First.fb2"
    second = tmp / "Second.fb2"
    body = '<FictionBook><body><section><p>First paragraph here.</p><p>Second paragraph here.</p><p>{}</p></section></body></FictionBook>'
    text.write_text(body.format("A"))
    second.write_text(body.format("B"))
    def pref(path):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        return tmp / f"state/botui/positions/{digest}.preferences"

    def terminal(path):
        return shelf.Terminal(tmp, binary, args=[str(path)])
    def close(t):
        t.run("kill-server", required=False)
    t = terminal(text)
    try:
        line_gap(t, 1)
        global_bytes = cfg.read_bytes()  # startup migrated the minimal config
        frame = open_settings(t)
        t.click("Paragraph spacing", frame)
        t.click("2 blank lines", t.see("2 blank lines"))
        frame = t.see("Use global settings for this book")
        t.click("Page width", frame)
        t.see("blank to use global")
        t.type("44")
        t.keys("Enter")
        t.see("44 cells")
        close_settings(t)
        line_gap(t, 2)
        t.keys("Space", "l", "w", "]", "Enter")
        t.wait("Quick width saved only for this edition", lambda f: json.loads(pref(text).read_text())["page_width"] != 44)
        assert cfg.read_bytes() == global_bytes
        t.keys("Space", "l", "w", "a")
        before = t.see("Text alignment")
        origin = next(line.index("First paragraph") for line in before.splitlines() if "First paragraph" in line)
        t.keys("Down")
        t.wait("Center preview moves the book text", lambda f: "Text alignment" in f and
               any(line.find("First paragraph") > origin for line in f.splitlines()))
        t.keys("Escape")
        t.wait("Cancel restores the original alignment", lambda f: "Text alignment" not in f and
               any(line.find("First paragraph") == origin for line in f.splitlines()))
        t.keys("Space", "l", "w", "a", "Down", "Enter")
        t.wait("Center saved for first book", lambda _: json.loads(pref(text).read_text()).get("alignment") == "center")
        assert cfg.read_bytes() == global_bytes
        # A deliberate edit in the all-books scope reaches the second book immediately.
        t.keys("F2")
        t.see("Appearance")
        t.keys("Right")
        t.click("Paragraph spacing", t.see("Paragraph spacing"))
        t.click("0 blank lines", t.see("0 blank lines"))
        t.see("[S This book]")
        t.keys("Escape")
        line_gap(t, 2)
        previous = tomllib.loads(global_bytes.decode())
        global_bytes = cfg.read_bytes()
        assert tomllib.loads(global_bytes.decode()) == {**previous, "paragraph_spacing": 0}
    finally:
        close(t)
    for path, gap in ((second, 0), (text, 2)):
        t = terminal(path)
        try:
            line_gap(t, gap)
            if path == second:
                assert not pref(path).exists()
            else:
                assert json.loads(pref(path).read_text())["alignment"] == "center"
                frame = open_settings(t)
                t.click("Paragraph spacing", frame)
                t.click("From book", t.see("From book"))
                close_settings(t)
                line_gap(t, 1)
                assert json.loads(pref(path).read_text())["paragraph_spacing"] == "from_book"
                frame = open_settings(t)
                t.click("Use global settings for this book", frame)
                close_settings(t)
                line_gap(t, 0)
                assert not pref(path).exists()
            assert cfg.read_bytes() == global_bytes
        finally:
            close(t)

    pdf = tmp / "Atlas.pdf"
    other = tmp / "Other.pdf"
    pdf.write_bytes(shelf.pdf())
    other.write_bytes(shelf.pdf(shades=(.2, .3, .4, .5)))
    t = terminal(pdf)
    try:
        t.wait("PDF first page", lambda f: "1 / " in f and "Loading" not in f)
        pdf_global = cfg.read_bytes()
        frame = open_settings(t)
        t.click("PDF page colors", frame)
        t.click("Invert original", t.see("Invert original"))
        close_settings(t)
        t.keys("Space", "t")
        t.see("● Invert original")
        t.keys("Home", "Down", "Right", "Enter")
        t.wait("quick palette stored on the PDF", lambda f: json.loads(pref(pdf).read_text())["pdf_theme"] == "default-dark")
        assert cfg.read_bytes() == pdf_global
    finally:
        close(t)
    for path, label in ((other, "● As in document"), (pdf, "● Invert original")):
        t = terminal(path)
        try:
            t.wait("PDF first page", lambda f: "1 / " in f and "Loading" not in f)
            t.keys("Space", "t")
            t.see(label)
            t.keys("Escape")
            assert cfg.read_bytes() == pdf_global
        finally:
            close(t)


with tempfile.TemporaryDirectory(prefix="botui-book-settings-") as folder:
    run(Path(folder))
print("Passed text and PDF settings isolation, inheritance, active-book controls, reopen and reset.")
