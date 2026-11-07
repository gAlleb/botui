#!/usr/bin/env python3
"""Run the BookOrbit shelf against a private loopback HTTP fixture and a tmux terminal."""

import io
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
API = "/reader/api/v1"
USER, PASSWORD, TOKEN = "shelf-test", "shelf-demo-pass", "shelf-demo-token"
COLLECTION = "Речная библиотека"
BOOK = "Два издания у маяка"
OTHER_BOOK = "Оттиск без обложки"
LIBRARIES = {31: "Главное книгохранилище", 32: "Архив оттисков"}
EPUB_MARK = "Электронный атлас пролива"
FB2_MARK = "Почта на пристани"
TOTAL = 101


def epub():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as archive:
        archive.writestr("mimetype", "application/epub+zip")
        archive.writestr("META-INF/container.xml", """<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
          <rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""")
        archive.writestr("book.opf", """<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Атлас пролива</dc:title><dc:identifier id="id">test-atlas</dc:identifier><dc:language>ru</dc:language></metadata>
          <manifest><item id="chapter" href="c.xhtml" media-type="application/xhtml+xml"/></manifest>
          <spine><itemref idref="chapter"/></spine></package>""")
        archive.writestr("c.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Атлас</title></head><body><h1>{EPUB_MARK}</h1><p>На карте показаны два маяка.</p></body></html>')
    return buf.getvalue()


class Fixture(ThreadingHTTPServer):
    def __init__(self, has_collections):
        super().__init__(("127.0.0.1", 0), Handler)
        self.has_collections = has_collections
        self.requests = []
        self.token = TOKEN
        self.refresh = "a" * 64
        self.expired_once = False
        self.progress = {}
        self.saved = []
        self.marks = {}
        self.files = {
            301: ("epub", "application/epub+zip", epub()),
            302: ("fb2", "application/x-fictionbook+xml", (ROOT / "examples/first-light.fb2").read_bytes()),
            303: ("pdf", "application/pdf", b"%PDF-1.4\n"),
        }

    def card(self, index):
        if index == 0:
            title = BOOK
            files = [{"id": i, "format": ext, "role": "primary" if i == 301 else "content", "sizeBytes": len(data)}
                     for i, (ext, _, data) in self.files.items()]
        else:
            title = OTHER_BOOK if index == 1 else f"Архивный том {index + 1}"
            files = [{"id": 4000 + index, "format": "pdf", "role": "primary", "sizeBytes": 1000}]
        return {"id": 200 + index, "title": title,
                "authors": ["Марина Лоцман"], "libraryId": 31 if index == 0 else 32, "status": "present", "files": files,
                "hasCover": index == 0, "coverVersion": "1", "readingProgress": 37 if index == 1 else None}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, code, body, mime="application/json"):
        if mime == "application/json":
            body = json.dumps(body, ensure_ascii=False).encode()
        self.server.requests.append((self.command, self.path, code))
        self.send_response(code)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def credentials(self):
        return {"accessToken": self.server.token, "refreshToken": self.server.refresh,
                "accessTokenExpiresAt": "2099-01-01T00:00:00Z",
                "refreshTokenExpiresAt": "2099-02-01T00:00:00Z", "sessionId": 8}

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        total = TOTAL if self.server.has_collections else 2
        catalogs = {API + "/books/query": range(total), API + "/libraries/31/books": [0],
                    API + "/libraries/32/books": range(1, total)}
        if self.path in catalogs:
            if self.headers.get("Authorization") != f"Bearer {self.server.token}":
                return self.reply(401, {"statusCode": 401})
            pagination = data.get("pagination", {})
            page, size = pagination.get("page"), pagination.get("size")
            if (not isinstance(page, int) or page < 0 or not isinstance(size, int) or
                    not 1 <= size <= 100 or self.headers.get_content_type() != "application/json"):
                return self.reply(400, {"message": "Invalid catalog query"})
            ids = list(catalogs[self.path])
            if q := data.get("q"):
                ids = [i for i in ids if q.casefold() in self.server.card(i)["title"].casefold() or q.casefold() in "марина лоцман"]
            if "filter" in data:
                assert data["filter"]["rules"][0]["value"] == ["reading", "rereading"]
                assert data["sort"][0] == {"field": "lastReadAt", "dir": "desc"}
                ids = [i for i in ids if i == 1]
            if data.get("sort") == [{"field": "addedAt", "dir": "desc"}]: ids.reverse()
            return self.reply(200, {"page": page, "size": size, "total": len(ids),
                                    "items": [self.server.card(i) for i in ids[page * size:(page + 1) * size]]})
        if self.path == API + "/books/200/bookmarks" and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            marks = self.server.marks.setdefault(data["bookFileId"], [])
            row = dict(id=max((m["id"] for m in marks),default=0)+1,cfi=data["cfi"],title=data["title"])
            marks.append(row)
            return self.reply(201,row)
        if self.path == API + "/auth/login":
            if (data.get("username"), data.get("password")) != (USER, PASSWORD):
                return self.reply(401, {"statusCode": 401, "message": "Invalid credentials"})
            if data.get("clientKind") != "native":
                return self.reply(400, {"message": "Expected native client"})
            if self.server.token is None:
                self.server.token, self.server.refresh = TOKEN, "a"*64
            return self.reply(200, {**self.credentials(), "user": {"id": 7, "username": USER}})
        if self.path in (API + "/auth/refresh", API + "/auth/logout"):
            if data != {"refreshToken": self.server.refresh}:
                return self.reply(401, {"statusCode": 401})
            if self.path.endswith("/refresh"):
                self.server.token, self.server.refresh = "rotated-access", "b" * 64
                return self.reply(200, self.credentials())
            self.server.token, self.server.refresh = None, None
            return self.reply(200, {})
        self.reply(404, {"path": self.path})

    def do_DELETE(self):
        if self.path.startswith(API + "/books/200/bookmarks/") and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            ident = int(self.path.rsplit("/",1)[1])
            for marks in self.server.marks.values(): marks[:] = [m for m in marks if m["id"] != ident]
            return self.reply(204,{})
        return self.reply(404,{})

    def do_PUT(self):
        if self.path != API + "/reading-progress" or self.headers.get("Authorization") != f"Bearer {self.server.token}":
            return self.reply(403,{})
        change,=json.loads(self.rfile.read(int(self.headers["Content-Length"])))["writes"]
        ident=change["fileId"]
        assert ident in (301,302,303) and change["kind"]=="text"
        assert change["editedAt"]==(self.server.progress.get(ident,{}).get("updatedAt") or "1970-01-01T00:00:00.000Z")
        assert change["text"].get("cfi","").startswith("epubcfi(") or change["text"].get("page",0)>0
        self.server.progress[ident]=dict(fileId=ident,updatedAt="2026-11-07T10:00:00.000Z",percentage=change["percentage"],text=change["text"])
        self.server.saved.append(ident)
        return self.reply(200,{"results":[{"fileId":ident,"status":"saved"}]})

    def do_GET(self):
        if self.headers.get("Authorization") != f"Bearer {self.server.token}":
            return self.reply(401, {"statusCode": 401})
        url = urlsplit(self.path)
        if url.path == API + "/books/200/bookmarks":
            return self.reply(200,self.server.marks.get(int(parse_qs(url.query)["readerFileId"][0]),[]))
        if url.path == API + "/reading-progress":
            ident=int(parse_qs(url.query)["fileId"][0])
            return self.reply(200,{"items":[self.server.progress.get(ident,dict(fileId=ident,updatedAt=None,percentage=0,text={"cfi":None,"page":None}))]})
        if url.path == API + "/libraries":
            total = TOTAL if self.server.has_collections else 2
            return self.reply(200, [{"id": i, "name": name, "bookCount": 1 if i == 31 else total - 1}
                                    for i, name in LIBRARIES.items()])
        if url.path == API + "/collections":
            return self.reply(200, [{"id": 91, "userId": 7, "name": COLLECTION, "bookCount": TOTAL}]
                              if self.server.has_collections else [])
        if url.path == API + "/collections/91/books" and self.server.has_collections:
            query = parse_qs(url.query)
            page, size = int(query.get("page", ["0"])[0]), int(query.get("size", ["50"])[0])
            if not (page >= 0 and 1 <= size <= 100):
                return self.reply(400, {"message": "Invalid pagination"})
            # Expire only the first request for the second page. The same request must be retried
            # with the new access token, and logout must revoke the matching rotated refresh token.
            if page == 1 and not self.server.expired_once:
                self.server.expired_once = True
                self.server.token = None
                return self.reply(401, {"statusCode": 401})
            indexes = range(page * size, min((page + 1) * size, TOTAL))
            return self.reply(200, {"page": page, "size": size, "total": TOTAL,
                                    "items": [self.server.card(i) for i in indexes]})
        if url.path == API + "/books/200/thumbnail":
            return self.reply(200, (ROOT / "scripts/fixtures/shelf-cover.jpg").read_bytes(), "image/jpeg")
        if url.path == API + "/books/201/thumbnail":
            return self.reply(404, {"statusCode": 404})
        for ident, (_, mime, data) in self.server.files.items():
            if url.path == API + f"/books/files/{ident}/serve":
                return self.reply(200, data, mime)
        self.reply(404, {"path": self.path, "statusCode": 404})


class Terminal:
    def __init__(self, tmp, binary, url=None, args=None):
        self.base = ["tmux", "-f", "/dev/null", "-S", str(tmp / "tmux.sock")]
        self.env = {"HOME": str(tmp), "PATH": os.environ["PATH"], "LC_ALL": "C.UTF-8",
                    "XDG_CONFIG_HOME": str(tmp / "config"), "XDG_STATE_HOME": str(tmp / "state"),
                    "XDG_CACHE_HOME": str(tmp / "cache")}
        self.run("new-session", "-d", "-s", "shelf", "-c", str(tmp), "-x", "115", "-y", "30",
                 shlex.join([str(binary), *(args if args is not None else ["--server",url])]))

    def run(self, *args, required=True):
        return subprocess.run(self.base + list(args), env=self.env, text=True, capture_output=True, check=required)

    def keys(self, *args):
        self.run("send-keys", "-t", "shelf", *args)

    def type(self, value):
        self.keys("-l", value)

    def wait(self, description, predicate):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            frame = self.run("capture-pane", "-p", "-t", "shelf", required=False)
            if frame.returncode:
                raise AssertionError(f"{description}: окно закрылось")
            if predicate(frame.stdout):
                return frame.stdout
            time.sleep(0.04)
        raise AssertionError(f"{description}\n{frame.stdout}")

    def see(self, label):
        return self.wait(label, lambda s: label.casefold() in s.casefold())

    def click(self, label, frame):
        row, line = next((i, line) for i, line in enumerate(frame.splitlines(), 1) if label in line)
        col = line.index(label) + 1
        seq = f"\033[<0;{col};{row}M\033[<0;{col};{row}m"
        self.keys("-H", *[f"{b:02x}" for b in seq.encode()])


def login(term, url, wrong_password=False):
    term.see("Connect to BookOrbit")
    term.keys("Enter")
    form = term.see("Password")
    assert url in form, form
    term.keys("Tab")
    term.keys("C-u")
    term.type(USER)
    term.keys("Tab")
    if wrong_password:
        term.type("incorrect")
        term.keys("Enter")
        term.wait("Отказ при ошибке входа", lambda s: "401" in s or "неверн" in s.lower())
        term.click("Password", term.see("Password"))
        term.keys("C-u")
    term.type(PASSWORD)
    term.keys("Enter")
    return term.wait("После входа открыты все книги", lambda frame: BOOK in frame and OTHER_BOOK in frame)


def formats(term):
    return term.wait("Выбор EPUB / FB2 / PDF", lambda s: sum(
        1 for line in s.splitlines()
        if any(label in line for label in ("EPUB", "FB2", "PDF"))) >= 3)


def quit_shelf(term):
    term.keys("q")
    deadline = time.monotonic() + 8
    while term.run("has-session", "-t", "shelf", required=False).returncode == 0:
        if time.monotonic() >= deadline:
            raise AssertionError("Сессия не закрылась после выхода из полки")
        time.sleep(0.04)


def check_collections(tmp, binary, fixture):
    url = f"http://127.0.0.1:{fixture.server_port}/reader"
    term = Terminal(tmp, binary, url)
    try:
        login(term, url, wrong_password=True)
        term.see("▀")  # Before opening files: real cover on the shelf.
        term.keys("v")
        term.see("[V Grid]")
        term.keys("/");term.see("Search all books");term.type("том 99");term.keys("Enter")
        term.see("Архивный том 99")
        term.keys("4");term.see("37%");assert OTHER_BOOK in term.see("Currently reading")
        term.keys("5");term.see(f"Архивный том {TOTAL}")
        term.keys("1");term.see(BOOK)
        term.click("[V Grid]", term.see("[V Grid]"))
        term.see("▀")
        term.keys("Right","Enter")
        term.wait("Сетка: соседняя книга", lambda s: OTHER_BOOK in s and BOOK not in s)
        term.keys("Escape")
        term.see("[V List]")
        term.keys("3")
        term.see(COLLECTION)
        term.keys("Enter")
        term.see(BOOK)
        term.keys("PageDown")
        term.wait("Следующая страница полки", lambda s: "Архивный том" in s and BOOK not in s)
        for _ in range(8):
            term.keys("PageDown")
            time.sleep(0.05)
        term.wait("Collection scrolled past first batch", lambda s: any(f"Архивный том {n}" in s for n in range(25,80)))
        term.keys("Home")
        term.see(BOOK)
        term.keys("Enter")

        formats(term)
        frame = term.see("▀")
        term.click("PDF", frame)
        term.wait("Сведения о PDF", lambda s: "PDF" in s and "Open the original pages" in s)
        term.keys("Escape")
        for label, heading in [("FB2", FB2_MARK), ("EPUB", EPUB_MARK)]:
            term.click(label, formats(term))
            term.see(heading)
            if label == "FB2":
                term.keys("z");term.wait("Zen",lambda s:"Z Zen" not in s);term.keys("o");term.see("Chapters:");term.keys("End","Enter")
                term.wait("Конец книги FB2",lambda s:s.strip().startswith("Обратный рейс"))
            term.keys("q")
            formats(term)
        term.keys("Escape")
        shelf = term.wait("Возврат к сетке", lambda s: BOOK in s and OTHER_BOOK in s)
        term.click(OTHER_BOOK, shelf)
        missing_cover = term.wait("Оттиск без обложки", lambda s: OTHER_BOOK in s and "PDF" in s and BOOK not in s)
        term.click("PDF", missing_cover)
        term.wait("Карточка PDF без обложки", lambda s: "Open the original pages" in s and "▀" not in s)
        term.keys("Escape")
        term.keys("Escape")
        term.see(BOOK)
        term.keys("Escape")
        term.see(COLLECTION)
        quit_shelf(term)
        assert fixture.saved==[302],fixture.saved
        requests = fixture.requests
        assert [status for method, path, status in requests if method == "POST" and path == API + "/auth/login"] == [401, 200], requests
        assert ("GET", API + "/books/200/thumbnail", 200) in requests, requests
        assert not any(path == API + "/books/201/thumbnail" for _, path, _ in requests), requests
        downloads = [path for method, path, status in requests if method == "GET" and path.endswith("/serve") and status == 200]
        assert downloads == [API + "/books/files/302/serve", API + "/books/files/301/serve"], requests
        assert [(path, status) for method, path, status in requests if method == "POST" and path.startswith(API + "/auth/")] == [
            (API + "/auth/login", 401), (API + "/auth/login", 200),
            (API + "/auth/refresh", 200), (API + "/auth/logout", 200)
        ], requests
        config = json.loads((tmp / "config/botui/connection.json").read_text())
        assert config == {"server": url, "username": USER}, "Проверьте сохраняемые поля подключения"
    finally:
        term.run("kill-server", required=False)


def check_without_collections(tmp, binary, fixture):
    url = f"http://127.0.0.1:{fixture.server_port}/reader"
    term = Terminal(tmp, binary, url)
    try:
        all_books = login(term, url)
        # These two titles belong to different libraries. Collections on this account are empty.
        term.run("resize-window","-t","shelf","-x","44","-y","16")
        term.see(BOOK)
        term.keys("Down","Enter")
        term.wait("В узкой сетке открылась вторая книга",lambda s:OTHER_BOOK in s and "[V " not in s)
        term.keys("Escape");term.see("[V List]")
        term.run("resize-window","-t","shelf","-x","115","-y","30")
        all_books=term.wait("После расширения видны обе книги",lambda s:BOOK in s and OTHER_BOOK in s)
        term.click(BOOK, all_books)
        term.click("EPUB", formats(term))
        term.see(EPUB_MARK)
        term.keys("q")
        formats(term)
        term.keys("Escape")
        term.wait("Возврат ко всем книгам", lambda s: BOOK in s and OTHER_BOOK in s)
        term.keys("2")
        for library_id, expected, other in [(31, BOOK, OTHER_BOOK), (32, OTHER_BOOK, BOOK)]:
            listing = term.wait("Выбор библиотеки", lambda s: all(name in s for name in LIBRARIES.values()))
            term.click(LIBRARIES[library_id], listing)
            term.wait(f"Книги библиотеки {library_id}", lambda s: expected in s and other not in s)
            term.keys("Escape")
        term.wait("Возврат к библиотекам", lambda s: all(name in s for name in LIBRARIES.values()))
        term.keys("3")
        term.wait("Пустой список коллекций", lambda s: "collections" in s.lower()
                  and ("no personal" in s.lower()) and BOOK not in s)
        term.keys("Escape")
        term.wait("Книги доступны после выхода из пустых коллекций", lambda s: BOOK in s and OTHER_BOOK in s)
        quit_shelf(term)
        requests = fixture.requests
        for path in ("/books/query", "/libraries/31/books", "/libraries/32/books"):
            assert ("POST", API + path, 200) in requests, requests
        for path in ("/libraries", "/collections", "/books/files/301/serve"):
            assert ("GET", API + path, 200) in requests, requests
        assert not any("/collections/" in path for _, path, _ in requests), requests
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/botui"
    if not binary.is_file() or not shutil.which("tmux"):
        sys.exit("Соберите botui и установите tmux перед запуском smoke-проверки.")
    for has_collections, check in ((False, check_without_collections), (True, check_collections)):
        with tempfile.TemporaryDirectory(prefix="botui-shelf-smoke-") as temp, Fixture(has_collections) as fixture:
            thread = threading.Thread(target=fixture.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
            thread.start()
            try:
                check(Path(temp), binary, fixture)
            finally:
                fixture.shutdown()
                thread.join()
    print("Пройдены все книги без коллекций, две отдельные библиотеки, коллекция, вход, обновление и закрытие сессии, пагинация, обложка, PDF, чтение FB2 и EPUB.")
