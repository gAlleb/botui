#!/usr/bin/env python3
"""A second device writes in the middle of a real TUI session. Its marker must survive."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import threading
import time
import zipfile
spec=importlib.util.spec_from_file_location("shelf",Path(__file__).with_name("smoke-shelf.py"))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def run(name,f):
    t=s.Terminal(Path(name),s.ROOT/'target/debug/botui',url:=f'http://127.0.0.1:{f.server_port}/reader')
    try:
        s.login(t,url);t.keys('Enter')
        t.click('FB2',s.formats(t));t.see(s.FB2_MARK)
        t.keys('Space', 'z');t.wait('Zen',lambda x:'Space z Zen' not in x);t.keys('Space', 'o');t.see('Sections:');t.keys('End','Enter');t.wait('переход',lambda x:x.strip().startswith('Обратный рейс'));t.keys('Space', 'q');s.formats(t)
        assert f.saved==[302],f.saved
        t.see("[H Put on hold]")
        # Erase only the isolated fixture's local position: the next jump must come from the API.
        previous,=(Path(name)/"state/botui/positions").glob("*.json")
        previous.unlink()
        f.marks[302]=[{"id":10,"title":"Last chapter on phone","cfi":f.progress[302]["text"]["cfi"]},{"id":11,"title":"Earlier edition","cfi":None}]
        t.click('FB2',s.formats(t));t.wait('возврат с сервера' ,lambda x:x.strip().startswith('Обратный рейс'))
        t.keys('Space', 'b', 'l');t.see("Last chapter on phone — Обратный рейс");t.see("Earlier edition · unavailable in this file");t.keys("Enter")
        t.keys('Space', 'b', 'a', 'Space', 'b', 'l');t.see("Bookmarks: 1");assert [m["id"] for m in f.marks[302]]==[11];t.keys("Escape");t.wait("Closed bookmarks",lambda x:"Bookmarks:" not in x);t.keys('Space', 'b', 'a', 'Space', 'b', 'l');t.see("Bookmarks: 2")
        assert f.marks[302][-1]["cfi"]==f.progress[302]["text"]["cfi"]
        t.keys("Escape")
        # Same CFI, newer edit timestamp; policy is conservative.
        f.progress[302]['updatedAt']='2026-11-07T11:00:00.000Z'
        t.keys('Space', 'o');t.see('Sections:');t.keys('Home','Enter');t.wait('первая глава',lambda x:x.strip().startswith(s.FB2_MARK));t.keys('Space', 'q');t.see('Another device updated')
        assert f.saved==[302] and f.progress[302]['updatedAt'].endswith('11:00:00.000Z')
        # An unsupported web position is never replaced by a local fraction.
        f.progress[302]['text']['cfi']='epubcfi(/6/999!/4/2/1:0)'
        t.click('FB2',s.formats(t));t.see(s.FB2_MARK);t.keys('Space', 'o');t.see('Sections:');t.keys('End','Enter')
        t.wait('переход без синхронизации',lambda x:x.strip().startswith('Обратный рейс'));t.keys('Space', 'q')
        t.see('could not be located');assert f.saved==[302]
        # Closing the reader and immediately leaving the shelf must finish its pending save.
        f.progress[302]['text']['cfi']=f.marks[302][-1]['cfi']
        t.click('FB2',s.formats(t));t.see('Обратный рейс');t.keys('Space', 'o');t.see('Sections:');t.keys('Home','Enter')
        t.see(s.FB2_MARK);t.keys('Space','q','q');s.all_books(t)
        assert f.saved==[302,302],f.saved
        s.source_home(t)
        before=sum(1 for method,path,status in f.requests if method=="POST" and path.endswith("/auth/login") and status==200)
        t.keys('b');t.see('Continue reading')
        assert sum(1 for method,path,status in f.requests if method=="POST" and path.endswith("/auth/login") and status==200)==before
        # Force the write to remain outstanding while Ctrl+C arrives. Logout must come afterwards.
        t.keys('1');t.click(s.BOOK,s.all_books(t));t.click('FB2',s.formats(t))
        t.see(s.FB2_MARK);t.keys('Space', 'o');t.see('Sections:');t.keys('End','Enter');t.see('Обратный рейс')
        f.save_entered=threading.Event();f.allow_save=threading.Event()
        t.keys('Space', 'q');assert f.save_entered.wait(4);t.keys('C-c')
        try:
            t.see('Saving position to BookOrbit');assert f.token is not None
        finally:
            f.allow_save.set()
        for _ in range(110):
            if t.run('has-session','-t','shelf',required=False).returncode: break
            time.sleep(.04)
        assert f.saved==[302,302,302] and f.token is None
    finally:t.run('kill-server',required=False)

def change_edition_during_save(tmp, server):
    url = f"http://127.0.0.1:{server.server_port}/reader"
    term = s.Terminal(Path(tmp), s.ROOT / "target/debug/botui", url)
    try:
        s.login(term, url)
        term.keys("Enter")
        term.click("FB2", s.formats(term))
        term.see(s.FB2_MARK)
        term.keys('Space', 'o')
        term.see("Sections:")
        term.keys("End", "Enter")
        term.see("Обратный рейс")
        server.save_entered = threading.Event()
        server.allow_save = threading.Event()
        term.keys('Space', 'q')
        assert server.save_entered.wait(4)
        try:
            card = term.see("Saving position to BookOrbit")
            term.click("EPUB", card)
            term.see("Saving position to BookOrbit")
            assert not server.saved
        finally:
            server.allow_save.set()
        term.see(s.EPUB_MARK)
        assert server.saved == [302]
    finally:
        term.run("kill-server", required=False)

def publisher_progress(tmp, server):
    """The HTTP double and the TUI independently resolve words in a merged cell."""
    import re
    words = " ".join(f"marker{i:03}" for i in range(500))
    cell_path = "/6/2!/4/4/2/2/4/1"
    markup = (
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Technical edition</title>'
        '<style>table{width:100%}.hidden{display:none}</style></head><body>'
        '<h1>Technical edition</h1><table><tbody><tr><td rowspan="2">One</td>'
        f'<td colspan="2">{words}</td></tr><tr><td>Two</td><td>Three</td></tr>'
        '</tbody></table><p class="hidden">HIDDEN TRAP</p></body></html>'
    )
    data = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(s.epub())) as source, zipfile.ZipFile(data, "w") as out:
        for member in source.namelist():
            out.writestr(member, markup if member == "c.xhtml" else source.read(member))
    server.files[301] = ("epub", "application/epub+zip", data.getvalue())
    marker = "marker147"
    server.progress[301] = {
        "fileId": 301,
        "percentage": 20,
        "updatedAt": "2026-11-08T10:00:00.000Z",
        "text": {"cfi": f"epubcfi({cell_path}:{words.index(marker)})"},
    }
    config = Path(tmp) / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('zen = true\nreading_layout = "scroll"\n')
    url = f"http://127.0.0.1:{server.server_port}/reader"
    t = s.Terminal(Path(tmp), s.ROOT / "target/debug/botui", url)
    try:
        s.login(t, url)
        t.keys("Enter")
        t.click("EPUB", s.formats(t))
        first = t.wait(
            f"BookOrbit position in a merged cell ({marker})",
            lambda frame: bool(frame.strip()) and marker in frame.strip().splitlines()[0],
        )
        for _ in range(4):
            t.keys("j")
            previous = first.strip().splitlines()[0]
            first = t.wait(
                "Scroll the merged cell",
                lambda frame: bool(re.search(r"marker\d+", frame))
                and frame.strip().splitlines()[0] != previous,
            )
        t.keys('Space', 'q')
        s.formats(t)
        t.wait("Published source position", lambda _: server.saved == [301])
        cfi = server.progress[301]["text"]["cfi"]
        prefix = f"epubcfi({cell_path}:"
        assert cfi.startswith(prefix) and cfi.endswith(")"), cfi
        offset = int(cfi[len(prefix):-1])
        restored = words[offset:].split()[0]
        assert words.index(marker) < offset and restored in first.strip().splitlines()[0], cfi
        for position in (Path(tmp) / "state/botui/positions").glob("*.json"):
            position.unlink()
        t.click("EPUB", s.formats(t))
        t.wait(
            "Reopen the saved word from the API",
            lambda frame: bool(frame.strip()) and restored in frame.strip().splitlines()[0],
        )
        t.keys('Space', 'q')
        s.formats(t)
    finally:
        t.run("kill-server", required=False)

def illustration_progress(tmp, server, pagination):
    """A device-local picture refines only its matching BookOrbit text position."""
    markup = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Picture resume</title></head>
      <body><h1>Opening chapter</h1><p>Local text before the plate.</p>
      <figure><img src="plate.png" style="width:120px;height:120px"/></figure>
      <p>Text after illustration.</p><p>Other device starts here.</p><p>Text on later pages.</p>
      </body></html>"""
    data = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(s.epub())) as source, zipfile.ZipFile(data, "w") as out:
        for name in source.namelist():
            content = source.read(name)
            if name == "book.opf":
                content = content.replace(
                    b"</manifest>", b'<item id="plate" href="plate.png" media-type="image/png"/></manifest>',
                )
            out.writestr(name, markup if name == "c.xhtml" else content)
        out.writestr("plate.png", s.ORIGINAL_COVER)
    book = data.getvalue()
    server.files[301] = ("epub", "application/epub+zip", book)
    root = Path(tmp)
    record = root / "state/botui/positions" / (hashlib.sha256(book).hexdigest() + ".json")
    record.parent.mkdir(parents=True)
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'zen = true\nreading_layout = "single"\npagination_mode = "{pagination}"\n')
    url = f"http://127.0.0.1:{server.server_port}/reader"
    args = ["NO_COLOR=1", str(s.ROOT / "target/debug/botui"), "--server", url]
    t = s.Terminal(root, Path("/usr/bin/env"), args=args)
    try:
        s.login(t, url)
        t.keys("Enter")
        for node, offset, hint in [
            ("4/1", 0, [0, 0]),
            ("10/1", 6, [0, 0]),
            ("4/1", 0, None),
        ]:
            local = {"version": 1, "block": 1, "byte": 0, "spread": False, "theme": "terminal"}
            if hint is not None:
                local["illustration"] = hint
            record.write_text(json.dumps(local))
            cfi = f"epubcfi(/6/2!/4/{node}:{offset})"
            server.progress[301] = {
                "fileId": 301, "percentage": 20, "updatedAt": "2026-11-08T10:00:00.000Z",
                "text": {"cfi": cfi},
            }
            t.click("EPUB", s.formats(t))
            page = t.see("Text on later pages.").strip()
            if pagination == "chapter":
                assert page.startswith("Opening chapter"), page
                paragraphs = ["Local text before the plate.", "Text after illustration.", "Other device starts here."]
                offsets = [page.index(text) for text in paragraphs]
                assert offsets == sorted(offsets)
            elif node == "10/1":
                assert page.startswith("device starts here."), page
            elif hint:
                # The terminal may render graphics, a block mosaic or a label.
                assert "Local text before the plate." not in page and "Text after illustration." in page, page
            else:
                assert page.startswith("Local text before the plate."), page
            t.keys("Space", "q")
            s.formats(t)
            assert not server.saved, server.saved
            assert server.progress[301]["text"]["cfi"] == cfi
            persisted = json.loads(record.read_text())
            assert (persisted["block"], persisted["byte"]) == (1 if node == "4/1" else 3, offset), persisted
            assert persisted.get("illustration") == (hint if node == "4/1" else None), persisted
    finally:
        t.run("kill-server", required=False)


if __name__=='__main__':
    for pagination in ("chapter", "keep_word"):
        with tempfile.TemporaryDirectory() as tmp, s.Fixture(False) as server:
            threading.Thread(target=server.serve_forever, daemon=True).start()
            try: illustration_progress(tmp, server, pagination)
            finally: server.shutdown()
    with tempfile.TemporaryDirectory() as tmp, s.Fixture(False) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try: change_edition_during_save(tmp, server)
        finally: server.shutdown()
    with tempfile.TemporaryDirectory() as tmp, s.Fixture(False) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try: publisher_progress(tmp, server)
        finally: server.shutdown()
    with tempfile.TemporaryDirectory() as tmp,s.Fixture(False) as f:
        threading.Thread(target=f.serve_forever,daemon=True).start()
        try:run(tmp,f)
        finally:f.shutdown()
    with tempfile.TemporaryDirectory() as tmp, s.Fixture(False) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        url = f"http://127.0.0.1:{server.server_port}/reader"
        term = s.Terminal(Path(tmp), s.ROOT / "target/debug/botui", url)
        try:
            s.login(term, url)
            term.keys("Enter")
            term.click("PDF", s.formats(term))
            term.see("1 / 3")
            term.keys("Right")
            term.wait("PDF page synced while open", lambda f: server.progress.get(303, {}).get("text", {}).get("page") == 2)
            server.progress[303] = dict(server.progress[303], text={"page": 3}, percentage=100)
            term.keys("-H", "1b", "5b", "49")
            term.see("Go to BookOrbit position")
            term.keys("Down", "Enter")
            term.see("2 / 3")
            term.keys('Space', 'q')
            s.formats(term)
            assert server.saved == [303] and server.progress[303]["text"]["page"] == 3
        finally:
            term.run("kill-server", required=False)
            server.shutdown()
    print('Проверены запись CFI для выбранного fileId, возврат из BookOrbit, конфликт и неизвестный CFI без перезаписи.')
