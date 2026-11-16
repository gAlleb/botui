#!/usr/bin/env python3
"""Search offline annotations and follow them into the exact edition from the start screen."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
binary = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(tmp):
    positions = tmp / "state/botui/positions"
    history = tmp / "state/botui/history"
    downloads = tmp / ".local/share/botui/downloads"
    for directory in (positions, history, downloads):
        directory.mkdir(parents=True)
    config = tmp / "config/botui"
    config.mkdir(parents=True)
    (config / "config.toml").write_text('reading_layout = "single"\n')
    text = tmp / "Journal.fb2"
    paragraphs = "".join(f"<p>Checkpoint {i:03} of the journey.</p>" for i in range(150))
    original = f"<FictionBook><body><section>{paragraphs}</section></body></FictionBook>"
    text.write_text(original)
    text_notes = positions / f"{digest(text)}.notes"
    # Legacy note arrays have no IDs. Finished entries are absent from Continue reading.
    text_notes.write_text(json.dumps([{"body": "Field observation", "block": 80, "byte": 0}]))
    stamp = {"secs_since_epoch": 0, "nanos_since_epoch": 0}
    (history / "seen.json").write_text(json.dumps({
        "path": str(text), "percent": 100, "modified": stamp, "read_at": stamp,
    }))
    pdf = downloads / "Atlas.pdf"
    pdf.write_bytes(shelf.pdf())
    (positions / f"{digest(pdf)}.notes").write_text(json.dumps({"notes": [{
        "id": "atlas", "block": 1, "byte": 0, "end": [1, 10], "quote": "Lighthouse atlas",
        "body": "", "color": "yellow", "pdf_regions": [{"page": 2, "bounds": [.03, .86, .5, .95]}],
    }], "sync": {}}))
    term = shelf.Terminal(tmp, binary, args=[])
    try:
        term.see("Highlights & notes")
        term.keys("a")
        term.see("2 results")
        term.type("observation")
        term.see("1 result")
        term.keys("Enter")
        frame = term.see("Checkpoint 080")
        assert "Checkpoint 000" not in frame, frame
        term.keys("Space", "q")
        term.see("2 results")
        term.type("lighthouse")
        term.click("Lighthouse atlas", term.see("1 result"))
        term.see("2 / 3")
        term.keys("Space", "q")
        term.see("2 results")
        term.type("Journal")
        term.see("1 result")
        text.write_text(original.replace("Checkpoint", "Revised"))
        term.keys("Enter")
        term.see("The book file has changed")
        term.keys("Escape")
        term.see("Unavailable edition")
        term.type("Field observation")
        term.see("1 result")
        term.keys("Enter")
        term.see("Open a local copy")
        text.write_text(original)
        term.keys("Escape")
        term.see("Journal.fb2")
        term.type("observation")
        term.see("1 result")
        text_notes.write_text("[]")
        term.keys("Enter")
        term.see("This annotation has been removed")
        term.keys("Escape")
        term.see("Atlas.pdf")
        term.keys("Escape")
        term.wait("Back on the start screen", lambda f: "Choose a library" in f and "Library highlights & notes" not in f)
    finally:
        term.run("kill-server", required=False)


with tempfile.TemporaryDirectory(prefix="botui-library-notes-") as folder:
    check(Path(folder))
print("Passed offline library annotation search, exact text/PDF jumps, missing editions and deleted notes.")
