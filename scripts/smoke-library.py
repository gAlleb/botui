#!/usr/bin/env python3
"""Select a folder in the UI, find a nested book, read it and reopen the saved library."""
import importlib.util
import tomllib
from pathlib import Path
import shutil
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", ROOT / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)


def check(tmp):
    lib = tmp / "Test library"
    chapter = lib / "Fiction" / "Роджер Желязны"
    chapter.mkdir(parents=True)
    filename = "Знак Единорога.fb2"
    shutil.copyfile(ROOT / "examples/first-light.fb2", chapter / filename)
    t = shelf.Terminal(tmp, ROOT / "target/debug/botui", args=["--local"])
    try:
        frame = t.see("Local library")
        assert frame.startswith("╭") and frame.splitlines()[0].endswith("╮"),frame
        t.click("U Borders",frame)
        t.wait("Square shelf corners",lambda f:f.startswith("┌") and f.splitlines()[0].endswith("┐"))
        t.keys("u")
        t.wait("Borderless shelf",lambda f:"Local library" in f.splitlines()[0] and not any(c in f for c in "╭╮┌┐"))
        t.keys("u")
        t.click("P Folder", t.wait("Rounded shelf restored",lambda f:f.startswith("╭")))
        t.see("Open library folder")
        t.type(str(lib))
        t.keys("Enter")
        t.see(str(lib))
        t.keys("/")
        t.see("Search local library")
        t.type("Знак Единорога")
        t.keys("Enter")
        t.see(filename)
        t.keys("v")
        t.see("[V List]")
        t.keys("Enter")
        t.see("Почта на пристани")
        t.keys("Right", "q")
        t.see("[V List]")
        t.keys("v", "2")
        t.see(filename)
        t.see("%")
        t.keys("3")
        t.see(filename)
        t.keys("q")
        for _ in range(100):
            if t.run("has-session",required=False).returncode:
                break
            time.sleep(.03)
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


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-library-") as folder:
        check(Path(folder))
    print("Passed direct folder entry, nested search, grid, reading history, recent files and saved root.")
