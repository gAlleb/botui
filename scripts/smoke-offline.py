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
        term.keys("q")
        shelf.formats(term)
        assert sum(p.endswith("/302/serve") for _, p, _ in server.requests) == 1
        shelf.quit_shelf(term)
        network = len(server.requests)
        term = shelf.Terminal(tmp, binary, args=[])
        term.click("BookOrbit offline downloads", term.see("BookOrbit offline downloads"))
        term.click(shelf.BOOK, term.see(shelf.BOOK))
        term.see(shelf.FB2_MARK)
        term.keys("o")
        term.wait("Contents overlay", lambda f:"[ Back ]" in f and "Chapters:" in f)
        term.keys("End", "Enter", "q")
        term.see("BookOrbit offline downloads")
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
    term.keys("o")
    term.see("Chapters:")
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
        term.click("EPUB", shelf.formats(term))
        term.wait("First Read opens EPUB", lambda f: f.strip().startswith("Chapter 1"))
        jump(term, 5)
        term.keys("q")
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
        frame = term.see(shelf.BOOK)
        assert "Only from OPDS" not in frame
        term.click(shelf.BOOK, frame)
        term.wait("Offline resumes chapter five", lambda f: f.strip().startswith("Chapter 5"))
        jump(term, 7)
        term.keys("q"); term.see("BookOrbit offline downloads")
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
        term.keys("q"); shelf.formats(term)
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
        term.keys("q"); term.see("BookOrbit offline downloads")
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

        # A different revision of the same length must replace the cached edition.
        updated = chapter_book("shore")
        assert len(updated) == len(epub.read_bytes())
        server.files[301] = ("epub", "application/epub+zip", updated)
        server.revision = "2"
        term.keys("b"); term.see("Continue reading"); term.keys("1")
        term.see(shelf.BOOK); term.keys("Enter")
        term.see("Harbor Press")
        term.click("EPUB", shelf.formats(term))
        term.wait("Updated edition resumes at chapter eight", lambda f: f.strip().startswith("Chapter 8") and "shore" in f)
        term.keys("q"); shelf.formats(term)
        index, = (downloads / ".editions").glob("*/301.json")
        assert (downloads / json.loads(index.read_text())["name"]).read_bytes() == updated
        assert [code for _, p, code in server.requests if p.endswith("/301/serve")] == [200, 304, 200]
        shelf.quit_shelf(term)
    finally:
        term.run("kill-server", required=False)
        server.shutdown()
print("Passed first Read, chapters 5 → 7 across an offline restart, one file transfer, separate OPDS shelf and background reconnect.")
