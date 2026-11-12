"""Download a native book, read it without a session, then reconnect to the original account."""
import importlib.util
import io
import json
import os
import tomllib
import shutil
import time
import zipfile
from pathlib import Path
import tempfile
import threading

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

with tempfile.TemporaryDirectory() as name, shelf.Fixture(False) as server:
    tmp = Path(name)
    binary = Path(os.environ.get("BOTUI_TEST_BIN", shelf.ROOT / "target/debug/botui"))
    url = f"http://127.0.0.1:{server.server_port}/reader"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    term = shelf.Terminal(tmp, binary, url)
    try:
        shelf.login(term, url)
        term.keys("Enter")
        shelf.formats(term)
        term.keys("Down", "d")
        term.see("Saved in BookOrbit offline downloads")
        # A pre-index Save offline is identified by its original account/file sidecar.
        downloads = tmp / ".local/share/botui/downloads"
        shutil.rmtree(downloads / ".editions")
        term.keys("r")
        term.see(shelf.FB2_MARK)
        term.keys("Space", "q")
        shelf.formats(term)
        assert sum(p.endswith("/302/serve") for _, p, _ in server.requests) == 1
        shelf.quit_shelf(term)
        network = len(server.requests)
        term = shelf.Terminal(tmp, binary, args=[])
        term.click("BookOrbit offline downloads", term.see("BookOrbit offline downloads"))
        term.click(shelf.BOOK, term.see(shelf.BOOK))
        term.see(shelf.FB2_MARK)
        term.keys("Space", "o")
        term.wait("Contents overlay", lambda f:"[ Back ]" in f and "Sections:" in f)
        term.keys("End", "Enter", "Space", "q")
        term.see("BookOrbit offline downloads")
        term.keys("DC"); term.see("Remove from this device?"); term.keys("Down", "Enter")
        term.wait("Book removed, queued move kept", lambda f: not list(downloads.glob("*.fb2")) and "Remove from this device?" not in f)
        term.keys("q")
        term.see("BookOrbit offline downloads")
        shelf.quit_shelf(term)
        assert network == len(server.requests)
        meta, = (tmp/".local/share/botui/downloads").glob(".*.json")
        record = json.loads(meta.read_text())
        assert record["pending"]["target"]["Text"].startswith("epubcfi(")
        term = shelf.Terminal(tmp, binary, url)
        shelf.login(term, url)
        assert server.saved == [302]
        assert server.progress[302]["text"]["cfi"] == record["pending"]["target"]["Text"]
        assert json.loads(meta.read_text())["pending"] is None
        shelf.quit_shelf(term)

        # The last screen can finish at the very same CFI as the saved position.
        completed = json.loads(meta.read_text())
        completed["base"]["percentage"] = 99
        server.progress[302]["percentage"] = 99
        completed["pending"] = {"target": record["pending"]["target"], "percentage": 100}
        meta.write_text(json.dumps(completed))
        term = shelf.Terminal(tmp, binary, url)
        shelf.login(term, url)
        assert server.saved == [302, 302] and server.progress[302]["percentage"] == 100
        assert json.loads(meta.read_text())["pending"] is None
        shelf.quit_shelf(term)
    finally:
        term.run("kill-server", required=False)
        server.shutdown()
print("Passed offline reading, conditional upload and finishing at an unchanged location.")

def chapter_book(place="coast"):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("mimetype", "application/epub+zip")
        z.writestr("META-INF/container.xml", '<container><rootfile full-path="book.opf"/></container>')
        manifest, spine = [], []
        for n in range(1, 10):
            manifest.append(f'<item id="c{n}" href="c{n}.xhtml" media-type="application/xhtml+xml"/>')
            spine.append(f'<itemref idref="c{n}"/>')
            lines = "".join(f"<p>Passage {i}. Walking along the {place}.</p>" for i in range(70))
            z.writestr(f"c{n}.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter {n}</h1>{lines}</body></html>')
        z.writestr("book.opf", '<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Voyage</dc:title></metadata><manifest>' + "".join(manifest) + '</manifest><spine>' + "".join(spine) + '</spine></package>')
    return buf.getvalue()


def jump(term, number):
    term.keys("Space", "o")
    term.see("Sections:")
    term.click(f"Chapter {number}", term.see(f"Chapter {number}"))
    term.wait("Reader opens selected chapter", lambda f: f.strip().startswith(f"Chapter {number}"))


with tempfile.TemporaryDirectory() as name, shelf.Fixture(False) as server:
    tmp = Path(name)
    binary = Path(os.environ.get("BOTUI_TEST_BIN", shelf.ROOT / "target/debug/botui"))
    url = f"http://127.0.0.1:{server.server_port}/reader"
    server.files[301] = ("epub", "application/epub+zip", chapter_book())
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = tmp / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('zen = true\nreading_layout = "single"\n')
    term = shelf.Terminal(tmp, binary, url)
    try:
        shelf.login(term, url)
        term.keys("Enter")
        # This scenario counts validations of a known revision. Opening before
        # metadata arrives may legitimately require a later If-None-Match.
        term.see("Harbor Press")
        term.click("EPUB", shelf.formats(term))
        term.wait("First Read opens EPUB", lambda f: f.strip().startswith("Chapter 1"))
        jump(term, 5)
        term.wait("Chapter five uploaded while reading", lambda f: "Chapter 5" in f and server.saved == [301])
        term.keys("Space", "q")
        shelf.formats(term)
        shelf.quit_shelf(term)
        downloads = tmp / ".local/share/botui/downloads"
        epub, = downloads.glob("*.epub")
        assert epub.read_bytes() == server.files[301][2] and server.saved == [301]
        meta, = downloads.glob(".*.json")
        baseline = json.loads(meta.read_text())["base"]
        assert baseline["text"]["cfi"] == server.progress[301]["text"]["cfi"] and baseline["updatedAt"] == server.progress[301]["updatedAt"], (baseline, server.progress[301])
        # A pre-existing OPDS download has no native account association.
        (downloads / "Only from OPDS.fb2").write_bytes(server.files[302][2])
        network = len(server.requests)
        term = shelf.Terminal(tmp, binary, args=[])
        term.see("BookOrbit offline downloads")
        term.keys("o")
        assert shelf.BOOK not in term.see("Only from OPDS")
        term.keys("q"); term.see("Choose a library"); term.keys("d")
        term.see(shelf.BOOK)
        term.keys("2")
        frame = term.wait("Connected progress on the offline Reading tab",
                          lambda f: shelf.BOOK in f and "Currently reading" in f and "%" in f)
        assert "Only from OPDS" not in frame
        term.click(shelf.BOOK, frame)
        term.wait("Offline resumes chapter five", lambda f: f.strip().startswith("Chapter 5"))
        jump(term, 7)
        term.keys("Space", "q"); term.see("BookOrbit offline downloads")
        term.keys("q"); term.wait("Back from offline Reading to downloads", lambda f: "BookOrbit offline downloads" in f and "Currently reading  ·" not in f)
        term.keys("q"); term.see("Choose a library")
        shelf.quit_shelf(term)
        pending = json.loads(meta.read_text())["pending"]
        assert network == len(server.requests)
        term = shelf.Terminal(tmp, binary, url)
        shelf.login(term, url)
        assert server.progress[301]["text"]["cfi"] == pending["target"]["Text"]
        term.keys("Enter")
        term.click("EPUB", shelf.formats(term))
        term.wait("BookOrbit resumes chapter seven", lambda f: f.strip().startswith("Chapter 7"))
        term.keys("Space", "q"); shelf.formats(term)
        term.keys("d"); term.see("Saved in BookOrbit offline downloads")
        served = [code for _, p, code in server.requests if p.endswith("/301/serve")]
        assert served == [200, 304], served
        shelf.quit_shelf(term)

        # Automatic reconnect with configured credentials while waiting on the home screen.
        connection = tomllib.loads(config.read_text())["bookorbit"]
        assert (connection["server"], connection["username"]) == (url, shelf.USER)
        config.write_text(
            'zen = true\nreading_layout = "single"\n[bookorbit]\n'
            f'server = "{url}"\nusername = "{shelf.USER}"\npassword = "{shelf.PASSWORD}"\n'
        )
        server.unavailable = True
        term = shelf.Terminal(tmp, binary, args=[])
        term.see("Choose a library"); term.keys("d"); term.click(shelf.BOOK, term.see(shelf.BOOK))
        term.wait("Saved EPUB opens during outage", lambda f: f.strip().startswith("Chapter 7"))
        jump(term, 8)
        term.keys("Space", "q"); term.see("BookOrbit offline downloads")
        term.keys("q"); term.see("Choose a library")
        end = time.monotonic() + 8
        while time.monotonic() < end and not any(code == 503 for _, _, code in server.requests): time.sleep(.05)
        assert any(code == 503 for _, _, code in server.requests), ("No reconnect attempted", json.loads(meta.read_text()), server.requests[-12:])
        move = json.loads(meta.read_text())["pending"]
        server.unavailable = False
        end = time.monotonic() + 15
        while time.monotonic() < end and json.loads(meta.read_text())["pending"]: time.sleep(.05)
        assert server.progress[301]["text"]["cfi"] == move["target"]["Text"]
        assert json.loads(meta.read_text())["pending"] is None
        assert [code for _, p, code in server.requests if p.endswith("/301/serve")] == served

        # The server is still in chapter 8 but its marker changed since the
        # download. Opening online must expose both positions before reading.
        local_move = {"target": pending["target"], "percentage": pending["percentage"]}
        conflict = json.loads(meta.read_text())
        conflict["pending"] = local_move
        meta.write_text(json.dumps(conflict))
        server.progress[301]["updatedAt"] = "2026-11-07T12:00:00.000Z"
        before = len(server.saved)
        term.keys("b"); term.see("Continue reading"); term.keys("1")
        shelf.all_books(term); term.keys("Enter")
        term.click("EPUB", shelf.formats(term)); term.see("Use offline position")
        term.keys("Escape"); shelf.formats(term)
        assert len(server.saved) == before and json.loads(meta.read_text())["pending"] == local_move
        term.click("EPUB", shelf.formats(term))
        term.click("Use offline position", term.see("Use offline position"))
        term.wait("Reconciliation opens offline chapter seven", lambda f: f.strip().startswith("Chapter 7"))
        assert server.progress[301]["text"]["cfi"] == local_move["target"]["Text"]
        assert json.loads(meta.read_text())["pending"] is None and len(server.saved) == before + 1
        term.keys("Space", "q"); shelf.formats(term)
        conflict = json.loads(meta.read_text())
        conflict["pending"] = local_move
        meta.write_text(json.dumps(conflict))
        server.progress[301]["text"] = {"cfi": move["target"]["Text"]}
        server.progress[301]["percentage"] = move["percentage"]
        server.progress[301]["updatedAt"] = "2026-11-07T13:00:00.000Z"
        term.click("EPUB", shelf.formats(term))
        term.click("Use BookOrbit position", term.see("Use BookOrbit position"))
        term.wait("Choosing BookOrbit opens chapter eight", lambda f: f.strip().startswith("Chapter 8"))
        assert len(server.saved) == before + 1 and json.loads(meta.read_text())["pending"] is None
        term.keys("Space", "q"); shelf.formats(term)
        shelf.source_home(term)

        # A different revision of the same length must replace the cached edition.
        updated = chapter_book("shore")
        assert len(updated) == len(epub.read_bytes())
        server.files[301] = ("epub", "application/epub+zip", updated)
        server.revision = "2"
        term.keys("b"); term.see("Continue reading"); term.keys("1")
        shelf.all_books(term); term.keys("Enter")
        term.see("Harbor Press")
        term.click("EPUB", shelf.formats(term))
        term.wait("Updated edition resumes at chapter eight", lambda f: f.strip().startswith("Chapter 8") and "shore" in f)
        chapter_eight = dict(server.progress[301])
        saves = len(server.saved)
        jump(term, 9)
        term.wait("Chapter nine uploaded while open", lambda f: len(server.saved) == saves + 1 and "Chapter 9" in f)
        server.progress[301] = dict(chapter_eight, text={"cfi": None}, percentage=0, updatedAt="2026-11-07T14:00:00.000Z")
        term.keys("-H", "1b", "5b", "49")
        term.see("Go to BookOrbit position · 0%")
        # The prompt stays open while the web reader turns pages in the background.
        server.progress[301] = chapter_eight
        label = f"Go to BookOrbit position · {chapter_eight['percentage']:.0f}%"
        frame = term.wait("Visible prompt refreshes without input", lambda f: label in f, timeout=20)
        # Another web write lands just before the click. A network error must
        # leave the reader in the dialog instead of jumping to the cached target.
        server.unavailable = True
        term.click(label, frame)
        term.see("BookOrbit unavailable · retry")
        server.progress[301] = dict(chapter_eight, text={"cfi": pending["target"]["Text"]},
                                    percentage=pending["percentage"], updatedAt="2026-11-07T15:00:00.000Z")
        server.unavailable = False
        term.keys("Enter")
        term.wait("Resume latest web chapter", lambda f: f.strip().startswith("Chapter 7"))
        term.keys("Space", "q"); shelf.formats(term)
        assert len(server.saved) == saves + 1
        index, = (downloads / ".editions").glob("*/301.json")
        assert (downloads / json.loads(index.read_text())["name"]).read_bytes() == updated
        assert [code for _, p, code in server.requests if p.endswith("/301/serve")] == [200, 304, 200]
        shelf.source_home(term); term.keys("d")
        term.see(shelf.BOOK); term.keys("DC"); term.see("Remove from this device?")
        copies = set(downloads.glob("*.epub"))
        assert len(copies) >= 1
        term.keys("Escape"); assert set(downloads.glob("*.epub")) == copies
        term.keys("DC"); term.see("Remove from this device?"); term.keys("Down","Enter")
        term.wait("Selected download disappears", lambda f: len(set(downloads.glob("*.epub"))) == len(copies) - 1 and "Remove from this device?" not in f)
        assert (downloads / "Only from OPDS.fb2").is_file()
        shelf.quit_shelf(term)
    finally:
        term.run("kill-server", required=False)
        server.shutdown()
print("Passed first Read, chapters 5 → 7 across an offline restart, one file transfer, separate OPDS shelf and background reconnect.")
