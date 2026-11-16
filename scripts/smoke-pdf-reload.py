#!/usr/bin/env python3
"""Replace a local PDF while it is open and check both the screen and edition-specific state."""
import hashlib
import importlib.util
import json
import tempfile
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("scroll", Path(__file__).with_name("smoke-scroll.py"))
scroll = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scroll)


def revision_state(root, data, suffix):
    digest = hashlib.sha256(data).hexdigest()
    return json.loads((root / "state/botui/positions" / (digest + suffix)).read_text())


def annotate(term, label):
    term.keys("Space", "b", "a")
    term.see("Page bookmarked")
    term.keys("Space", "a", "p")
    term.see("Page note")
    term.type(label)
    term.keys("Enter")


def replace(file, data):
    tmp = file.with_suffix(".tmp")
    tmp.write_bytes(data)
    tmp.replace(file)


def main():
    with tempfile.TemporaryDirectory(prefix="botui-pdf-reload-") as tmp:
        root = Path(tmp)
        file = root / "draft.pdf"
        old = scroll.s.pdf(True, shades=(.3, .8, .5))
        new = scroll.s.pdf(True, shades=(.8, .3, .5, .4))
        short = scroll.s.pdf(True, shades=(.8,))

        # Save a different note and bookmark in the edition that will replace the open file.
        file.write_bytes(new)
        term = scroll.start(root, file)
        try:
            term.see("1 / 4")
            annotate(term, "Revised edition")
            scroll.stop(term)
        finally:
            term.run("kill-server", required=False)

        file.write_bytes(old)
        term = scroll.start(root, file)
        try:
            term.see("1 / 3")
            term.keys("Space", "l", "1", "Right")
            term.see("2 / 3")
            annotate(term, "Original edition")
            term.keys("Space", "l", "3", "+", "Down", "Down")
            original = term.wait("Zoomed second sheet", lambda f:
                                 "2 / 3" in f and "110%" in f and "#" * 20 in f)
            pixels = [line for line in original.splitlines() if "#" * 20 in line]
            # Let the watcher observe an incomplete rewrite, including the missing EOF.
            file.write_bytes(new[:-60])
            time.sleep(1.7)
            frame = term.see("2 / 3")
            assert any(line in frame for line in pixels), frame

            term.keys("F2")
            term.see("Appearance")
            replace(file, new)
            time.sleep(1.4)
            term.see("Appearance")
            term.keys("Escape")
            term.wait("Replacement painted at same page and zoom", lambda f:
                      "2 / 4" in f and "110%" in f and ":" * 20 in f)
            term.keys("Space", "b", "l")
            marks = term.see("Page bookmarks")
            assert "Page 1" in marks and "Page 2" not in marks, marks
            term.keys("Escape")
            term.keys("Space", "a", "a")
            term.see("Revised edition")
            term.keys("Escape")

            # Refresh under the reading menu; shrinking clamps the current page.
            term.keys("Space", "m")
            term.see("Continuous scroll")
            replace(file, short)
            term.wait("Menu survives reload", lambda f:
                      "Continuous scroll" in f and "1 / 1" in f)
            term.keys("Escape")
            term.wait("Short document keeps zoom", lambda f: "1 / 1" in f and "110%" in f)
            scroll.stop(term)
        finally:
            term.run("kill-server", required=False)
        first = revision_state(root, old, ".json")
        second = revision_state(root, new, ".json")
        third = revision_state(root, short, ".json")
        assert first["marks"] == [[1, 0]] and second["marks"] == [[0, 0]] and not third["marks"]
        assert first["block"] == second["block"] == 1 and third["block"] == 0
        assert first["scan_offset"] > 0 and first["scan_offset"] == second["scan_offset"]
        assert "Original edition" in str(revision_state(root, old, ".notes"))
        assert "Revised edition" in str(revision_state(root, new, ".notes"))
    print("PDF reload: partial writes, replaced rasters, menu/settings, page/zoom/scroll and edition notes passed")


if __name__ == "__main__":
    main()
