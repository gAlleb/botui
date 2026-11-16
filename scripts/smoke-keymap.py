#!/usr/bin/env python3
"""Run navigation aliases through the same reading, shelf and dialog loops people use."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
binary = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()


def start(home, *args):
    return shelf.Terminal(home, binary, args=[str(arg) for arg in args])


def check_text(home, book, configured):
    t = start(home, book)
    try:
        first = t.see("Passage 001")
        if configured:
            t.keys("x")
            t.wait("reading alias turns the page", lambda f: "Passage 001" not in f and "Passage " in f)
            t.keys("z")
            t.see("Passage 001")
        else:
            t.keys("x")
            t.wait("unbound x leaves page intact", lambda f: f == first)
        t.keys("Space", "f")
        t.see("Find:")
        t.type("xzx")
        t.see("1 matches")
        t.keys("Escape")
        t.keys("Space", "m")
        t.see("Previous page")
        t.keys("x" if configured else "Escape")
        t.wait("close reading menu", lambda f: "Previous page" not in f)
        if configured:
            t.keys("Space", "c")
            t.see("NORMAL")
            t.keys("x")
            t.see("VISUAL")
            t.keys("Escape")
            t.see("NORMAL")
            t.keys("m", "x")
            t.see("Mark x saved")
            t.keys("Space", "a", "n")
            t.see("Ctrl+S Save")
            t.type("xzx note")
            t.keys("C-s")
            t.see("Annotation saved")
            notes = next((home / "state/botui/positions").glob("*.notes"))
            assert json.loads(notes.read_text())[0]["body"] == "xzx note"
    finally:
        t.run("kill-server", required=False)


with tempfile.TemporaryDirectory(prefix="botui-keymap-") as temp:
    root = Path(temp)
    library = root / "books"
    library.mkdir()
    book = library / "Study.fb2"
    book.write_text("<FictionBook><body><section><p>xzx marker.</p>" +
                    "".join(f"<p>Passage {n:03}.</p>" for n in range(100)) +
                    "</section></body></FictionBook>")
    for configured in [False, True]:
        home = root / ("custom" if configured else "defaults")
        cfg = home / "config/botui/config.toml"
        cfg.parent.mkdir(parents=True)
        cfg.write_text('reading_layout = "single"\n' + ('''
[keys]
z = "previous"
[keymap.reading]
x = "Right"
z = "Right"
f = "Right"
[keymap.vim]
x = "v"
[keymap.dialogs]
x = "Esc"
[keymap.library]
x = "Enter"
[keymap.pdf]
x = "+"
[keymap.images]
x = "Right"
''' if configured else ""))
        check_text(home, book, configured)
    t = start(home, "--local", library)
    try:
        t.see("Study")
        t.keys("x")
        t.see("Passage 001")
    finally:
        t.run("kill-server", required=False)
    # The login form handles its own text fields. No request is submitted.
    t = start(home)
    try:
        t.see("Connect to BookOrbit")
        t.keys("Home", "Down")
        t.see("Sign in to a BookOrbit server")
        t.keys("Enter")
        t.see("Password")
        t.keys("Tab", "C-u")
        t.type("xzx")
        t.see("Username   xzx")
        t.keys("Tab", "x")
        t.see("••••••••")
        t.keys("BSpace")
        t.wait("physical x was one password character", lambda f: "Password" in f and "••••••••" not in f)
    finally:
        t.run("kill-server", required=False)
    pdf = root / "study.pdf"
    pdf.write_bytes(shelf.pdf())
    t = start(home, pdf)
    try:
        t.see("1 / 3")
        t.keys("x")
        t.see("110%")
    finally:
        t.run("kill-server", required=False)
    cbz = root / "study.cbz"
    with zipfile.ZipFile(cbz, "w") as archive:
        for i in range(3):
            archive.write(shelf.ROOT / "scripts/fixtures/shelf-cover.jpg", f"{i}.jpg")
    t = start(home, cbz)
    try:
        t.see("1 / 3")
        t.keys("x")
        t.see("2 / 3")
    finally:
        t.run("kill-server", required=False)
print("Navigation keys checked in text, Vim, library, dialogs, PDF and comics.")
