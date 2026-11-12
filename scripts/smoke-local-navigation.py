#!/usr/bin/env python3
"""Exercise the real local folder tree, its keyboard/mouse focus and compact tabs."""
import importlib.util
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("splitters", ROOT / "scripts/smoke-splitters.py")
splitters = importlib.util.module_from_spec(spec)
spec.loader.exec_module(splitters)
shelf = splitters.shelf


def check_back(home, grid):
    library = home / "Library"
    part = library / "Vol-39" / "Part"
    part.mkdir(parents=True)
    for i in range(39):
        (library / f"Vol-{i:02}").mkdir()
    book = "Lighthouse.fb2"
    shutil.copyfile(ROOT / "examples/first-light.fb2", part / book)
    config = home / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f"local_grid = {str(grid).lower()}\nzen = true\n")
    term = shelf.Terminal(home, ROOT / "target/debug/botui", args=["--local", str(library)])
    try:
        term.see("Vol-00/")
        term.keys("End"); term.see("Vol-39/")
        term.keys("Enter"); term.see("Part/")
        term.keys("Enter"); term.wait("Book in nested folder", lambda f: book in f or "До первого парома" in f)
        term.keys("Enter"); term.see("Почта на пристани")
        term.keys("q"); term.wait("Reader returns to its folder", lambda f: "Local library" in f and (book in f or "До первого парома" in f))
        term.keys("q"); term.see("Part/")
        term.keys("q"); term.see("Vol-39/")
        # The selected folder is still visible; Enter must reopen it without scrolling.
        term.keys("Enter"); term.see("Part/")
        term.keys("3"); term.wait("Recent shelf", lambda f: "Recently added" in f and (book in f or "До первого парома" in f))
        term.keys("q"); term.see("Part/")
        term.click("[ Q Back ]", term.see("[ Q Back ]")); term.see("Vol-39/")
        term.keys("q"); term.see("Choose a library")
        shelf.quit_shelf(term)
    finally:
        term.run("kill-server", required=False)


def check(home):
    library = home / "Library"
    amber = library / "Fiction" / "Amber"
    amber.mkdir(parents=True)
    for i in range(32):
        (library / f"Vol-{i:02}").mkdir()
    name = "Lighthouse.fb2"
    shutil.copyfile(ROOT / "examples/first-light.fb2", amber / name)
    term = shelf.Terminal(home, ROOT / "target/debug/botui", args=["--local", str(library)])
    try:
        term.run("resize-window", "-t", "shelf", "-x", "80", "-y", "24")
        frame = splitters.at_width(term, 20)
        assert "Navigation · F6" in frame and "2 Reading" in frame and "3 Recently added" in frame

        # Expand Fiction separately from opening it. No read of its siblings is required.
        term.keys("F6", "Home", "Down", "Down", "Right")
        frame = term.see("▸ Amber")
        assert name not in frame
        term.keys("Right", "Enter")
        term.see(name)
        term.keys("Enter")
        term.see("Почта на пристани")
        term.keys("Right", "Space", "q")
        term.see(name)
        term.keys("a"); term.see("New collection…"); term.keys("Enter")
        term.see("New collection name"); term.type("Night shelf"); term.keys("Enter")
        term.see("Collection updated.")
        term.keys("4"); term.click("Night shelf", term.see("Night shelf"))
        term.wait("Local collection contains the original", lambda f: name in f and "Collection · Night shelf" in f)
        term.click("[A Collections]", term.see("[A Collections]"))
        term.click("Remove from Night shelf", term.see("Remove from Night shelf"))
        term.see("This collection is empty.")
        assert (amber / name).is_file()
        term.click("2 Reading", term.see("2 Reading"))
        term.wait("Reading shelf", lambda s: name in s and "%" in s)
        term.keys("a"); term.click("Add to Night shelf", term.see("Add to Night shelf"))
        term.see("Collection updated."); term.keys("4")
        term.click("Night shelf", term.see("Night shelf"))
        term.click("[Del Delete collection]", term.see("[Del Delete collection]"))
        assert "Night shelf" in term.see("Delete collection?")
        term.keys("Enter"); term.see("Collection · Night shelf")
        term.keys("DC"); term.see("Delete collection?"); term.keys("Right", "Enter")
        term.wait("Original folder still contains book", lambda f: name in f and "Fiction/Amber" in f)
        term.keys("4"); assert "Night shelf" not in term.see("New collection…")
        term.keys("Escape"); term.see(name); term.keys("Enter"); term.see("Sections: 3")
        term.keys("q"); term.see(name)
        term.click("3 Recently added", term.see("3 Recently added"))
        term.wait("Recently added shelf", lambda s: "Recently added" in s and name in s)

        term.click("1 Folders", term.see("1 Folders"))
        term.wait("Return to current folder", lambda s: "Fiction/Amber" in s and name in s)

        # Mouse disclosure collapses all folders without covering the other shortcuts.
        term.click("▾ 1 Folders", term.see("▾ 1 Folders"))
        term.wait("Collapsed tree", lambda s: "▸ 1 Folders" in s and "Vol-00" not in s)
        term.click("▸ 1 Folders", term.see("▸ 1 Folders"))
        term.click("Vol-00", term.see("Vol-00"))
        term.see("No books or folders here.")
        term.keys("BSpace")
        term.see("Fiction/")

        splitters.size(term, 140)
        splitters.at_width(term, 28)
        splitters.change_width(term, home, "sidebar_width", 28, 36)
        term.keys("F6", "End", "Up", "Up", "Up")
        term.see("Vol-31")
        frame = term.see("3 Recently added")
        assert "2 Currently reading" in frame and "1 Folders" in frame
        term.keys("Enter")
        term.see(str(library / "Vol-31"))
        term.keys("r")
        term.see("Library saved")
        term.run("resize-window", "-t", "shelf", "-x", "42", "-y", "14")
        frame = term.wait("Narrow tabs", lambda s: "2 Reading" in s and "3 Recent" in s)
        assert "Navigation · F6" not in frame
    finally:
        term.run("kill-server", required=False)
    term = shelf.Terminal(home, ROOT / "target/debug/botui", args=["--local"])
    try:
        splitters.size(term, 140)
        frame = splitters.at_width(term, 36)
        assert str(library / "Vol-31") in frame and "· Vol-31" in frame
        assert "Vol-00" not in frame
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    for grid in [False, True]:
        with tempfile.TemporaryDirectory(prefix="botui-local-back-") as tmp:
            check_back(Path(tmp), grid)
    with tempfile.TemporaryDirectory(prefix="botui-local-tree-") as tmp:
        check(Path(tmp))
    print("Passed Q/Back through nested folders and shelves, saved selection, folder expansion, mouse/keyboard opening, resize and saved root.")
