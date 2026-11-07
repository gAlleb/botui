#!/usr/bin/env python3
"""Check native and OPDS transfers against an EPUB served by a private loopback fixture.
Run with an optional large EPUB path to verify the installed download path and SHA-256.
"""
import argparse
import hashlib
import importlib.util
import json
import shutil
import tempfile
import threading
import time
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

class Handler(shelf.Handler):
    def do_GET(self):
        if self.path == "/opds":
            return self.reply(200, json.dumps({
                "metadata": {"title": "Download test catalog"},
                "publications": [{"metadata": {"title": "Transfer test edition"},
                                  "links": [{"rel": "http://opds-spec.org/acquisition",
                                             "type": "application/epub+zip", "href": "/edition"}]}],
            }).encode(), "application/opds+json")
        if self.path not in ("/edition", shelf.API + "/books/files/301/serve"):
            return super().do_GET()
        native = self.path != "/edition"
        if native and self.headers.get("Authorization") != "Bearer " + self.server.token:
            return self.reply(401,{})
        self.send_response(200)
        self.send_header("Content-Type", "application/epub+zip")
        if native:
            self.send_header("Content-Length", str(self.server.file.stat().st_size))
        self.end_headers()
        # Hold the socket open after one visible chunk until the TUI has painted it.
        gate = self.server.resume
        try:
            with self.server.file.open("rb") as book:
                self.wfile.write(book.read(self.server.first_chunk))
                self.wfile.flush()
                if not gate.wait(timeout=12):
                    return
                shutil.copyfileobj(book, self.wfile, length=256*1024)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The cancellation check intentionally closes the client mid-response.

def size_label(size):
    if size < 1024:
        return f"{size} B"
    if size < 1024 * 1024:
        return f"{(size + 1023) // 1024} KiB"
    return f"{size / (1024 * 1024):.1f} MiB"

def check(file, tmp, binary):
    with file.open("rb") as src:
        digest = hashlib.file_digest(src, "sha256").hexdigest()
    with shelf.Fixture(False) as server:
        server.file = file
        server.first_chunk = min(2 << 20, file.stat().st_size // 2)
        server.RequestHandlerClass = Handler
        card = server.card
        def edition(i):
            result = card(i)
            if i == 0:
                result["files"][0]["sizeBytes"] = file.stat().st_size
            return result
        server.card = edition
        threading.Thread(target=server.serve_forever,daemon=True).start()
        origin = f"http://127.0.0.1:{server.server_port}"
        try:
            for source in ("bookorbit", "opds"):
                home = tmp/source
                home.mkdir()
                url = origin+("/reader" if source=="bookorbit" else "/opds")
                args = ["--server" if source=="bookorbit" else "--opds",url]
                t = shelf.Terminal(home,binary,args=args)
                try:
                    if source=="bookorbit":
                        shelf.login(t,url)
                        t.keys("Enter")
                        shelf.formats(t)
                    else:
                        t.see("OPDS catalog")
                        t.keys("Tab","Tab","Enter")
                        t.click("Transfer test edition", t.see("Transfer test edition"))
                        t.see("Save offline")
                    if source == "bookorbit":
                        status = f"{size_label(server.first_chunk)} / {size_label(file.stat().st_size)}"
                    else:
                        status = f"{size_label(server.first_chunk)} received"

                    # The worker may be blocked on a slow read. The UI must still accept Esc
                    # and return to the edition before that read completes.
                    server.resume = threading.Event()
                    t.keys("d")
                    t.see(status)
                    t.keys("Escape")
                    t.see("Save offline")
                    if source == "bookorbit":
                        # This requires a new HTTP request on the same account.
                        t.keys("3")
                        t.wait("Read another shelf during cancellation", lambda screen:
                               "Recently added" in screen and shelf.OTHER_BOOK in screen)
                        t.keys("1")
                        t.see(shelf.BOOK)
                        t.keys("Enter")
                        shelf.formats(t)
                    server.resume.set()
                    abandoned = home / "cache/botui/downloads"
                    deadline = time.monotonic() + 6
                    while list(abandoned.glob("edition-*")) and time.monotonic() < deadline:
                        time.sleep(.05)
                    assert not list(abandoned.glob("edition-*")), "cancelled transfer left a partial file"
                    assert not list((home/".local/share/botui/downloads").glob("*.epub"))

                    server.resume = threading.Event()
                    t.keys("d")
                    t.see(status)
                    server.resume.set()
                    end = time.monotonic()+90
                    while time.monotonic()<end:
                        screen=t.run("capture-pane","-p","-t","shelf").stdout
                        if "Saved in Offline downloads" in screen:
                            break
                        time.sleep(.1)
                    else:
                        raise AssertionError(screen)
                    downloaded, = (home/".local/share/botui/downloads").glob("*.epub")
                    assert downloaded.stat().st_size==file.stat().st_size
                    with downloaded.open("rb") as saved:
                        assert hashlib.file_digest(saved,"sha256").hexdigest()==digest
                    with zipfile.ZipFile(downloaded) as archive:
                        assert archive.read("mimetype").strip()==b"application/epub+zip"
                        assert "META-INF/container.xml" in archive.namelist()
                    assert not list((home/"cache/botui/downloads").glob("edition-*"))
                    if file.stat().st_size > 1 << 20:
                        config = home / "config/botui/config.toml"
                        config.write_text("max_download_mib = 1\n")
                        t.keys("d")
                        t.see("1 MiB download limit")
                        assert list((home / ".local/share/botui/downloads").glob("*.epub")) == [downloaded]
                        assert not list(abandoned.glob("edition-*"))
                    print(f"{source}: displayed “{status}”, cancelled and retried; {file.stat().st_size} bytes, matching SHA-256, valid EPUB",flush=True)
                finally:
                    if hasattr(server, "resume"):
                        server.resume.set()
                    t.run("kill-server",required=False)
                    shutil.rmtree(home)
        finally:
            server.shutdown()

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edition",nargs="?",type=Path)
    parser.add_argument("--binary",type=Path,default=shelf.ROOT/"target/debug/botui")
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="download-check-",dir=shelf.ROOT/"target") as folder:
        tmp=Path(folder)
        book=args.edition
        if book is None:
            book=tmp/"fixture.epub"
            book.write_bytes(shelf.epub())
            with zipfile.ZipFile(book, "a") as archive:
                archive.writestr("fixture.bin", bytes(4 << 20), zipfile.ZIP_STORED)
        check(book,tmp,args.binary.resolve())
