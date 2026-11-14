#!/usr/bin/env python3
"""Preview and save independent PDF colors and palette in the reader and F2."""
import importlib.util
import os
import tempfile
import tomllib
from pathlib import Path

spec = importlib.util.spec_from_file_location("sources", Path(__file__).with_name("smoke-sources.py"))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
binary = Path(os.environ.get("BOTUI_BINARY", s.ROOT / "target/debug/botui")).resolve()

with tempfile.TemporaryDirectory(prefix="botui-scan-theme-") as tmp:
    root = Path(tmp)
    book = root / "paper.pdf"
    book.write_bytes(s.pdf(True, shades=(0, 1, 0.5)))
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('scan_theme = "mist-dark"\ntheme = "sepia-dark"\nopaque_background = true\n')

    def saved():
        settings = tomllib.loads(config.read_text())
        assert settings["theme"] == "sepia-dark"
        return settings["scan_theme"], settings["scan_page_colors"]

    def pixels(color):
        ansi = t.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
        return color in ansi

    def picker():
        t.keys("Space", "t")
        return t.see("Page colors")

    t = s.shelf.Terminal(root, binary, args=[str(book)])
    try:
        t.see("1 / 3")
        assert saved() == ("mist-dark", "theme")
        frame = picker()
        assert "● Use reading theme" in frame
        t.click("Invert original", frame)
        t.see("● Invert original")
        t.wait("white inverted page with Mist margins", lambda _: pixels("255;255;255") and pixels("58;49;80"))
        t.keys("Escape")
        t.wait("cancel restores theme", lambda f: "Page colors" not in f and pixels("199;182;221") and not pixels("255;255;255"))
        assert saved() == ("mist-dark", "theme")
        picker()
        t.keys("[", "Enter")
        t.wait("mode saved independently", lambda _: saved() == ("mist-dark", "inverted"))
        picker()
        t.keys("Home", "Down", "Down", "Down", "Left", "Enter")
        t.wait("palette saved independently", lambda _: saved() == ("sepia-light", "inverted"))
        t.wait("white page with Sepia margins", lambda _: pixels("255;255;255") and pixels("241;232;208"))
        t.keys("F2")
        t.see("Appearance")
        t.keys("Right", "End", "Up", "Enter")
        frame = t.see("As in document")
        t.click("As in document", frame)
        t.wait("F2 changed mode only", lambda _: saved() == ("sepia-light", "original"))
        t.keys("Up", "Enter")
        t.see("PDF reading theme")
        t.keys("Home", "Down", "Down", "Right", "Enter")
        t.wait("F2 changed palette only", lambda _: saved() == ("gray-dark", "original"))
        t.keys("Escape")
        t.wait("PDF restored", lambda f: "1 / 3" in f and pixels("68;68;68") and not pixels("255;255;255"))
    finally:
        t.run("kill-server", required=False)
    t = s.shelf.Terminal(root, binary, args=[str(book)])
    try:
        t.see("1 / 3")
        t.run("resize-window", "-t", "shelf", "-x", "54", "-y", "16")
        t.wait("resized original page", lambda f: len(f.splitlines()) == 16 and pixels("0;0;0"))
        assert "● As in document" in picker()
        t.keys("]", "]", "Enter")
        t.wait("restored palette used by themed page", lambda _: saved() == ("gray-dark", "theme") and pixels("198;198;198"))
    finally:
        t.run("kill-server", required=False)
print("PASS independent PDF page pixels and margins, live preview, cancel, F2, resize and reopen")
