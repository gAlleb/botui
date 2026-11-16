#!/usr/bin/env python3
"""Select a folder in the UI, find a nested book, read it and reopen the saved library."""
import importlib.util
import tomllib
import re
from pathlib import Path
import shutil
import zipfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", ROOT / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)


def check(tmp):
    binary = ROOT / "target/debug/botui"
    home = shelf.Terminal(tmp, binary, args=[])
    try:
        for width, height in [(120, 38), (80, 24), (42, 14)]:
            home.run("resize-window", "-t", "shelf", "-x", str(width), "-y", str(height))
            home.keys("Home")
            # Wait for the resize to repaint before measuring text rows.
            frame = home.wait("local first at new size", lambda f: len(f.splitlines()) == height
                              and "Browse a folder on this device" in f and "/  Search" in f)
            assert frame.index("Browse local library") < frame.index("Connect to BookOrbit")
            hint_row = next(i for i, line in enumerate(frame.splitlines()) if "/  Search" in line)
            for desc in ["Sign in to a BookOrbit", "Connect to an OPDS", "Continue your BookOrbit", "Read books you saved"]:
                home.keys("Down")
                frame = home.see(desc)
                assert "/  Search" in frame.splitlines()[hint_row], (width, height, frame)
        home.keys("Home", "Enter")
        home.see("Local library")
        home.keys("q")
        home.see("Choose a library")
        home.keys("End")
        home.see("Read books you saved from OPDS")
        home.keys("Enter")
        home.see("OPDS downloads")
    finally:
        home.run("kill-server",required=False)
    lib = tmp / "Test library"
    chapter = lib / "Fiction" / "Роджер Желязны"
    chapter.mkdir(parents=True)
    filename = "Знак Единорога.fb2"
    shutil.copyfile(ROOT / "examples/first-light.fb2", chapter / filename)
    orphan = chapter / "a015.fb2"
    orphan.write_bytes('<?xml version="1.0" encoding="windows-1251"?><FictionBook><description><title-info><author><first-name>Виктор</first-name><last-name>Пелевин</last-name></author><book-title>Жёлтая стрела</book-title><sequence name="Северное депо"/></title-info></description><body><section><p>Поезд номер 15.</p></section></body></FictionBook>'.encode("cp1251"))
    with zipfile.ZipFile(chapter/"b018.epub","w") as z:
        z.writestr("META-INF/container.xml", '<container><rootfile full-path="book.opf"/></container>')
        z.writestr("book.opf", '<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Star map</dc:title><dc:creator>Aria Nova</dc:creator></metadata></package>')
    t = shelf.Terminal(tmp, ROOT / "target/debug/botui", args=["--local"])
    try:
        frame = t.see("Local library")
        assert frame.startswith("╭") and frame.splitlines()[0].endswith("╮"),frame
        t.keys("u")
        t.wait("Square shelf corners",lambda f:f.startswith("┌") and f.splitlines()[0].endswith("┐"))
        t.keys("u")
        t.wait("Borderless shelf",lambda f:"Local library" in f.splitlines()[0] and not any(c in f for c in "╭╮┌┐"))
        t.keys("u")
        t.click("P Folder", t.wait("Rounded shelf restored",lambda f:f.startswith("╭")))
        t.see("Open library folder")
        t.type(str(lib))
        t.keys("Enter")
        t.see(str(lib))
        for query, expected in [("Пелевин Желтая", "Жёлтая стрела"), ("северное депо", "a015.fb2"), ("Nova map", "b018.epub")]:
            t.keys("/");t.see("Search local library");t.type(query)
            t.wait(expected, lambda f: expected in shelf.search_body(f, "Search local library") and query in f)
            t.keys("Escape");t.wait("Back on shelf", lambda f: str(lib) in f and "Search local library" not in f)
        t.keys("/")
        t.see("Search local library")
        heading = "Search local library"
        t.type("8")
        t.wait("first character filters before Enter", lambda f: "1 result" in f and "b018.epub" in f and filename not in f)
        for width, height in [(52, 16), (115, 30)]:
            t.run("resize-window", "-t", "shelf", "-x", str(width), "-y", str(height))
            frame = t.wait("Resized search keeps its query and visible shelf",
                           lambda f: len(f.splitlines()) == height and len(f.splitlines()[0]) == width
                           and "1 result" in shelf.search_body(f, heading)
                           and " / 8" in shelf.search_body(f, heading) and "Local library" in f.splitlines()[0])
            left, top, right, bottom = shelf.search_box(frame, heading)
            assert 0 < left and 0 < top and abs(left - (width - right)) <= 1 and abs(top - (height - bottom)) <= 1, frame
        t.keys("BSpace")
        t.type("з")
        t.see(filename)  # first keystroke, no submit
        t.type("нк единр")
        t.see(filename)  # live fuzzy results before pressing Enter
        t.type("ххх")
        t.see("No matches.")
        t.keys("BSpace", "BSpace", "BSpace")
        t.see(filename)
        t.keys("Enter")
        t.see("Почта на пристани")
        t.keys("Right", "Space", "q")
        t.see(filename)
        t.keys("Escape"); t.wait("Back on shelf", lambda f: str(lib) in f and "Search local library" not in f)
        t.keys("2")
        t.see(filename)
        t.see("%")
        t.keys("3")
        t.see(filename)
        t.keys("q");t.wait("Recent shelf returns to the folder", lambda f: str(lib) in f and "Fiction/" in f)
        t.keys("q");t.see("Choose a library");t.keys("q")
        for _ in range(100):
            if t.run("has-session",required=False).returncode:
                break
            time.sleep(.03)
        t = shelf.Terminal(tmp, binary, args=["--resume"])
        t.see("Space z Zen");t.keys("Space", "q")
        t.run("kill-server",required=False)
        settings = tomllib.loads((tmp / "config/botui/config.toml").read_text())
        assert settings["library_root"] == str(lib), settings
        t = shelf.Terminal(tmp, ROOT / "target/debug/botui", args=["--local"])
        t.see(str(lib))
        t.click("Fiction/", t.see("Fiction/"))
        t.click("R Set library", t.see("R Set library"))
        t.see("Library saved")
        assert Path(tomllib.loads((tmp / "config/botui/config.toml").read_text())["library_root"]) == lib / "Fiction"
    finally:
        t.run("kill-server", required=False)


def check_roles(root, binary):
    # A selection and an open tab are both visible even when a narrow shell hides its rail.
    for palette, accent in [("catppuccin-mocha", (203, 166, 247)),
                            ("catppuccin-latte", (136, 57, 239))]:
        tmp = root / palette
        library = tmp / "library"
        (library / "Alpha").mkdir(parents=True)
        (library / "Bravo").mkdir()
        config = tmp / "config/botui/config.toml"
        config.parent.mkdir(parents=True)
        config.write_text(f'interface_theme = "{palette}"\nopaque_background = true\nlocal_grid = false\n')
        term = shelf.Terminal(tmp, binary, args=["--local", str(library)])
        try:
            term.run("resize-window", "-x", "46", "-y", "16")
            term.see("1 Folders")

            def role(word):
                ansi = term.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
                flags, color = set(), None
                for token in re.finditer(r"\x1b\[([0-9;]*)m|([^\x1b]+)", ansi):
                    if token[2] is not None:
                        if word in token[2]:
                            return flags, color
                        continue
                    codes = iter(int(c or 0) for c in token[1].split(";"))
                    for c in codes:
                        if c == 0:
                            flags, color = set(), None
                        elif c in (1, 2, 4, 7):
                            flags.add(c)
                        elif c in (22, 24, 27):
                            flags.difference_update({22: (1, 2), 24: (4,), 27: (7,)}[c])
                        elif c in (38, 48):
                            mode = next(codes)
                            rgb = tuple(next(codes) for _ in range(3 if mode == 2 else 1))
                            if c == 38:
                                color = rgb
                        elif c == 39:
                            color = None
                raise AssertionError((word, ansi))

            assert role("1 Folders") == ({1, 4}, accent)
            assert role("Alpha/") == ({7}, accent)
            assert role("Bravo/")[0] == set()
            assert role(" Q Back ") == (set(), accent)
            term.keys("Down")
            term.wait("Cursor moves off the first folder", lambda _: role("Bravo/") == ({7}, accent))
            term.keys("F2")
            term.see("Style")
            assert role("Style") == ({1, 4}, accent)
            assert role("Esc Back") == (set(), accent)
        finally:
            term.run("kill-server", required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-library-") as folder:
        check(Path(folder))
        check_roles(Path(folder), ROOT / "target/debug/botui")
    print("Passed direct folder entry, nested search, grid, reading history, recent files and saved root.")
