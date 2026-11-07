#!/usr/bin/env python3
"""Check notices in the installed-style executable, including the Legal link offline."""
import importlib.util
import os
import subprocess
import tempfile
import time
from pathlib import Path

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", root / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
exe = Path(os.environ.get("BOTUI_TEST_BIN", root / "target/debug/botui"))
notice = subprocess.check_output([exe, "--license"], text=True)
assert (root / "LICENSE").read_text() in notice
assert (root / "licenses/bookorbit/ADDITIONAL_TERMS.md").read_text() in notice
assert (root / "licenses/bookorbit/LICENSE").read_text() in notice

with tempfile.TemporaryDirectory(prefix="botui-about-") as tmp:
    d = Path(tmp)
    opener = d / "xdg-open"
    opener.write_text('#!/bin/sh\nprintf "%s" "$1" > "$HOME/opened-url"\n')
    opener.chmod(0o755)
    t = shelf.Terminal(d, Path("/usr/bin/env"), args=[f"PATH={tmp}:{os.environ['PATH']}", str(exe)])
    try:
        t.see("Your TUI book reader.")
        def corners(frame):
            return [(x,y) for y,line in enumerate(frame.splitlines())
                    for x,glyph in enumerate(line) if glyph in "╭╮╰╯"]
        def stable_tabs(width, height):
            t.run("resize-window", "-t", "shelf", "-x", str(width), "-y", str(height))
            bounds = corners(t.see("Interface theme"))
            assert len(bounds) == 4, bounds
            for label in ["Reading theme", "Local folder", "Password command", "Keyboard shortcuts",
                          "Terminal book reader", "Bauhaus fallback covers", "Interface theme"]:
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
        assert not (d / "opened-url").exists()
        t.click(" Legal ", frame)
        frame = t.see("BOTUI · GNU GPL")
        assert "Powered by BookOrbit" not in frame
        t.click("Artwork notice", frame)
        frame = t.see("Powered by BookOrbit")
        assert "Bauhaus fallback cover generator" in frame and "Copyright (C) 2025-2026 neon" in frame
        t.click("Powered by BookOrbit", frame)
        for _ in range(40):
            if (d / "opened-url").exists():
                break
            time.sleep(.05)
        assert (d / "opened-url").read_text() == "https://github.com/bookorbit/bookorbit"
        t.run("resize-window", "-t", "shelf", "-x", "42", "-y", "14")
        t.keys("End")
        t.see("--license")
        t.keys("Escape")
        t.see("BOTUI · GNU GPL")
        t.keys("Enter")
        t.see("Powered by BookOrbit")
    finally:
        t.run("kill-server", required=False)
print("Passed: both license texts, accessible About tab, scrolling and upstream attribution link.")
