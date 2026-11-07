#!/usr/bin/env python3
"""Check notices in the installed-style executable, including the About link offline."""
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
exe = root / "target/debug/botui"
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
        t.keys("?")
        t.click("About", t.see("Keyboard shortcuts"))
        frame = t.see("Powered by BookOrbit")
        assert "BOTUI" in frame and "Copyright (C) 2025-2026 neon" in frame
        t.click("Powered by BookOrbit", frame)
        for _ in range(40):
            if (d / "opened-url").exists():
                break
            time.sleep(.05)
        assert (d / "opened-url").read_text() == "https://github.com/bookorbit/bookorbit"
        t.run("resize-window", "-t", "shelf", "-x", "42", "-y", "14")
        t.keys("End")
        t.see("--license")
    finally:
        t.run("kill-server", required=False)
print("Passed: both license texts, accessible About tab, scrolling and upstream attribution link.")
