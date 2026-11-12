#!/usr/bin/env python3
"""Check notices in the installed-style executable, including the offline Legal tab."""
import importlib.util
import os
import subprocess
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", root / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
exe = Path(os.environ.get("BOTUI_TEST_BIN", root / "target/debug/botui"))
notice = subprocess.check_output([exe, "--license"], text=True)
assert (root / "LICENSE").read_text() in notice
assert notice.startswith("BOTUI · GPL-3.0-or-later")

with tempfile.TemporaryDirectory(prefix="botui-about-") as tmp:
    d = Path(tmp)
    t = shelf.Terminal(d, exe, args=[])
    try:
        t.see("Your TUI book reader.")
        def corners(frame):
            return [(x,y) for y,line in enumerate(frame.splitlines())
                    for x,glyph in enumerate(line) if glyph in "╭╮╰╯"]
        def stable_tabs(width, height):
            t.run("resize-window", "-t", "shelf", "-x", str(width), "-y", str(height))
            w, h = min(width, 106), min(height, 34)
            x, y = (width-w)//2, (height-h)//2
            bounds = [(x,y), (x+w-1,y), (x,y+h-1), (x+w-1,y+h-1)]
            t.wait("resized settings panel", lambda f: "Interface theme" in f and corners(f) == bounds)
            for label in ["Reading theme", "Local folder", "Password command", "Word definitions", "Keyboard shortcuts",
                          "Terminal book reader", "BOTUI · GNU GPL", "Interface theme"]:
                t.keys("Tab")
                assert corners(t.see(label)) == bounds, (width, height, label)
        t.keys("F2")
        stable_tabs(130, 42)
        stable_tabs(72, 22)
        t.keys("?")
        t.click("About", t.see("Keyboard shortcuts"))
        frame = t.see("Terminal book reader")
        assert "local folder" in frame and "OPDS" in frame and "Powered by" not in frame
        t.keys("Enter")
        t.see("Terminal book reader")
        t.click(" Legal ", frame)
        frame = t.see("BOTUI · GNU GPL")
        assert "botui --license" in frame and "BookOrbit" not in frame.split("BOTUI · GNU GPL", 1)[1]
        t.run("resize-window", "-t", "shelf", "-x", "42", "-y", "14")
        t.keys("End")
        t.see("--license")
        t.keys("Escape")
        t.see("Choose a library")
    finally:
        t.run("kill-server", required=False)
print("Passed: bundled GPL, compact Legal tab and stable settings geometry.")
