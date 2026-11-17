#!/usr/bin/env python3
"""Exercise Vim motions through the real reader and capture only its clipboard output."""
import html
import importlib.util
import os
from pathlib import Path
import tempfile
import time
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()
WORDS = "Alpha.one Beta-two Gamma,three final."

CASES = [
    # This pre-existing Unicode word boundary is deliberately preserved.
    ("word_control", WORDS, "yiw", "Alpha.one"),
    ("W", WORDS, "Wvly", "Be"),
    ("B", WORDS, "$Bvly", "fi"),
    ("E", WORDS, "Evly", "e "),
    ("inner_WORD", WORDS, "yiW", "Alpha.one"),
    ("around_WORD", WORDS, "yaW", "Alpha.one "),
    ("f", WORDS, "f.vly", ".o"),
    ("F", WORDS, "$F.vly", ".o"),
    ("t", WORDS, "t.vly", "a."),
    ("T", WORDS, "$T.vly", "on"),
    ("repeat", WORDS, "f.;vly", "."),
    ("reverse", WORDS, "f.;,vly", ".o"),
    ("repeat_till", WORDS, "t.;vly", "l."),
    ("counted_find", WORDS, "2f.vly", "."),
    ("find_missing", WORDS, "3f.vly", "Al"),
    ("find_space", WORDS, "f vly", " B"),
    ("visual_literal_y", "abycy", "vfyy", "aby"),
    ("yank_find", WORDS, "yf.", "Alpha."),
    ("yank_till", WORDS, "yt.", "Alpha"),
    ("inner_quote", 'Prefix "quoted words" suffix.', 'f"yi"', "quoted words"),
    ("around_quote", 'Prefix "quoted words" suffix.', 'f"ya"', '"quoted words"'),
    ("counted_quote", 'Prefix "one" and "two".', 'f"2yi"', '"one"'),
    ("inner_single", "Prefix 'single words' suffix.", "f'yi'", "single words"),
    ("inner_backtick", "Prefix `code words` suffix.", "f`yi`", "code words"),
    ("inner_round", "Prefix (outer [inner] end) suffix.", "f(yi(", "outer [inner] end"),
    ("inner_square", "Prefix (outer [inner] end) suffix.", "f[yi[", "inner"),
    ("around_curly", "Prefix {curly words} suffix.", "f{ya{", "{curly words}"),
    ("inner_angle", "Prefix <angle words> suffix.", "f<yi<", "angle words"),
]


def fixture(path, text):
    with zipfile.ZipFile(path, "w") as book:
        book.writestr("mimetype", "application/epub+zip")
        book.writestr("META-INF/container.xml", '''<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
          <rootfiles><rootfile full-path="book.opf"/></rootfiles></container>''')
        book.writestr("book.opf", '''<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Vim fixture</dc:title>
          <dc:identifier id="id">vim-fixture</dc:identifier><dc:language>en</dc:language></metadata>
          <manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest>
          <spine><itemref idref="one"/></spine></package>''')
        book.writestr("one.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body><p>'
                      + html.escape(text) + '</p></body></html>')


def check(root, name, text, command, expected):
    book = root / "book.epub"
    fixture(book, text)
    helpers = root / "bin"
    helpers.mkdir()
    # Never touch the user's clipboard, regardless of the platform/provider.
    for provider in ("wl-copy", "xclip", "xsel", "pbcopy", "termux-clipboard-set"):
        helper = helpers / provider
        helper.write_text('#!/bin/sh\n[ "$1" = "--version" ] && exit 0\ncat > "$HOME/copied"\n')
        helper.chmod(0o755)
    copied = root / "copied"
    term = shelf.Terminal(root, Path("/usr/bin/env"), args=[
        f"PATH={helpers}{os.pathsep}{os.environ['PATH']}", str(BINARY), str(book),
    ])
    try:
        term.see(text.split()[0])
        term.keys("Space", "c")
        term.see("NORMAL")
        term.type("gg0")
        term.type(command)
        # A clipboard process may create its file before finishing the write.
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if copied.exists() and copied.read_text() == expected:
                return
            time.sleep(.04)
        actual = copied.read_text() if copied.exists() else None
        frame = term.run("capture-pane", "-p", "-t", "shelf").stdout
        raise AssertionError(f"{name}: {command!r} copied {actual!r}, expected {expected!r}\n{frame}")
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-vim-") as tmp:
        for name, text, command, expected in CASES:
            root = Path(tmp) / name
            root.mkdir()
            check(root, name, text, command, expected)
    print(f"Passed {len(CASES)} Vim cursor, find/repeat, WORD, text-object and clipboard scenarios.")
