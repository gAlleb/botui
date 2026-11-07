#!/usr/bin/env python3
"""Run the BookOrbit shelf against a private loopback HTTP fixture and a tmux terminal."""

import base64
import hashlib
import io
import itertools
import tomllib
import json
import os
import re
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
# A valid full-size PNG distinct from the 24×36 JPEG thumbnail.
ORIGINAL_COVER = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAGAAAACQCAIAAAB4aPl2AAAA/0lEQVR42u3csRFAQBBAUWeEaqALmRKMRAFKU4BE"
    "G8pQgwJUYAMBM+799LI3u9ndpbYfCt1XIgAECBAgQIAAZVoVnC3TmYnCvNYmyIoBAgQIECBAAgQIECBAgAABEiBA"
    "gAABAgQIECABelpyR9EEAQIECBAgQIAECBAgQIAAAQIkQIAAAQIECBAgAQIECBAgQIAAARIgQIAAAQIECJAAAQL0"
    "VtE/is3YZaJwbLsJsmKAAAECBAiQAAECBAgQIECABAgQIECAAAECBEiAAAECBAgQIEACBAgQIECAAAESIECAAAEC"
    "BAgQIAECBOijolfPwVtgEyRAgAABAgQI0N+7AOpFCtQBnTkqAAAAAElFTkSuQmCC"
)


def pdf(tones=False, width=300, height=440, shades=(0.3, 0.8, 0.5)):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', f'<< /Type /Pages /Count {len(shades)} /Kids [{" ".join(f"{3+2*i} 0 R" for i in range(len(shades)))}] >>'.encode()]
    for i in range(len(shades)):
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {4+2*i} 0 R >>'.encode())
        content = f'0.{i+2} 0.4 0.6 rg {28*i} 30 {220-i*28} {350-i*40} re f'.encode()
        if tones:content=f'{shades[i]} g 0 0 1000 1000 re f'.encode()
        if i==1 and not tones:content+=b" BT /F1 16 Tf 10 32 Td (Lighthouse atlas) Tj ET"
        objects.append(f'<< /Length {len(content)} >>\nstream\n'.encode()+content+b'\nendstream')
    data=bytearray(b'%PDF-1.4\n'); offsets=[0]
    for i,obj in enumerate(objects,1):
        offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
    offset=len(data);data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for pos in offsets[1:]:data.extend(f'{pos:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode());return data


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
        self.personal = {}
        self.requests = []
        self.token = TOKEN
        self.refresh = "a" * 64
        self.expired_once = False
        self.statuses = {201:"reading"}
        self.progress = {}
        self.saved = []
        self.marks = {}
        self.files = {
            301: ("epub", "application/epub+zip", epub()),
            302: ("fb2", "application/x-fictionbook+xml", (ROOT / "examples/first-light.fb2").read_bytes()),
            303: ("pdf", "application/pdf", pdf()),
            4001: ("pdf", "application/pdf", pdf()),
        }

    def card(self, index):
        if index == 0:
            title = BOOK
            files = [{"id": i, "format": ext, "role": "primary" if i == 301 else "content", "sizeBytes": len(data)}
                     for i, (ext, _, data) in self.files.items() if i in (301, 302, 303)]
        else:
            title = OTHER_BOOK if index == 1 else f"Архивный том {index + 1}"
            files = [{"id": 4000 + index, "format": "pdf", "role": "primary", "sizeBytes": 1000}]
        return {"id": 200 + index, "title": title,
                "authors": ["Марина Лоцман"], "libraryId": 31 if index == 0 else 32, "status": "present", "files": files,
                "hasCover": index == 0, "coverVersion": "1", "readingProgress": 37 if index == 1 else None}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, code, body, mime="application/json", etag=None):
        if mime == "application/json":
            body = json.dumps(body, ensure_ascii=False).encode()
        self.server.requests.append((self.command, self.path, code))
        self.send_response(code)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(body)))
        if etag: self.send_header("ETag", etag)
        self.end_headers()
        self.wfile.write(body)

    def credentials(self):
        return {"accessToken": self.server.token, "refreshToken": self.server.refresh,
                "accessTokenExpiresAt": "2099-01-01T00:00:00Z",
                "refreshTokenExpiresAt": "2099-02-01T00:00:00Z", "sessionId": 8}

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
        total = TOTAL if self.server.has_collections else 2
        catalogs = {API + "/books/query": range(total), API + "/libraries/31/books": [0],
                    API + "/libraries/32/books": range(1, total)}
        if self.path == API + "/collections":
            assert data["icon"] == "FolderOpen" and data["isPublic"] is False
            ident = 100 + len(self.server.personal)
            self.server.personal[ident] = (data["name"], set())
            return self.reply(201, {"id":ident, "name":data["name"], "bookCount":0})
        if self.path == API + "/collections/100/books":
            self.server.personal[100][1].update(data["bookIds"])
            return self.reply(201, {})
        if self.path == API + "/books/201/reset-reading-state" and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            self.server.statuses[201]="unread"
            return self.reply(201,{"readStatus":{"status":"unread","startedAt":None,"finishedAt":None}})
        if self.path == API + "/dashboard/widgets/batch" and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            return self.reply(200, {"items":[{"type":"reading-streak","failed":False,"data":{"currentStreak":4,"longestStreak":9}},
                                  {"type":"monthly-challenge","failed":False,"data":{"title":"Streak Builder","progress":4,"target":5}}]})
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
                ids=[i for i in ids if self.server.statuses.get(200+i) in ("reading","rereading")]
            if data.get("sort") == [{"field": "addedAt", "dir": "desc"}]: ids.reverse()
            return self.reply(200, {"page": page, "size": size, "total": len(ids),
                                    "items": [self.server.card(i) for i in ids[page * size:(page + 1) * size]]})
        if self.path == API + "/books/200/bookmarks" and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            marks = self.server.marks.setdefault(data["bookFileId"], [])
            row = dict(id=max((m["id"] for m in marks),default=0)+1,cfi=data["cfi"],title=data["title"])
            marks.append(row)
            return self.reply(201,row)
        if self.path == API + "/auth/login":
            if getattr(self.server, "unavailable", False): return self.reply(503, {})
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
        if self.path == API + "/collections/100":
            if self.headers.get("Authorization") != f"Bearer {self.server.token}":
                return self.reply(401, {})
            if self.server.deny_collection_delete:
                return self.reply(403, {})
            del self.server.personal[100]
            return self.reply(204, b"", "application/octet-stream")
        if self.path == API + "/collections/100/books":
            data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            self.server.personal[100][1].difference_update(data["bookIds"])
            return self.reply(200, {})

        if self.path.startswith(API + "/books/200/bookmarks/") and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            ident = int(self.path.rsplit("/",1)[1])
            if ident in getattr(self.server, "denied_marks", set()):
                return self.reply(403,{})
            if not any(m["id"] == ident for marks in self.server.marks.values() for m in marks):
                return self.reply(404,{})
            for marks in self.server.marks.values(): marks[:] = [m for m in marks if m["id"] != ident]
            return self.reply(204,{})
        return self.reply(404,{})

    def do_PATCH(self):
        if self.path == API + "/books/201/status" and self.headers.get("Authorization") == f"Bearer {self.server.token}":
            st=json.loads(self.rfile.read(int(self.headers['Content-Length'])))['status']
            assert st in ('reading','on_hold')
            self.server.statuses[201]=st
            return self.reply(200,{"status":st,"startedAt":"2026-10-01","finishedAt":None})
        self.reply(404,{})
    def do_PUT(self):
        if self.path != API + "/reading-progress" or self.headers.get("Authorization") != f"Bearer {self.server.token}":
            return self.reply(403,{})
        change,=json.loads(self.rfile.read(int(self.headers["Content-Length"])))["writes"]
        ident=change["fileId"]
        assert ident in (301,302,303) and change["kind"]=="text"
        assert change["editedAt"]==(self.server.progress.get(ident,{}).get("updatedAt") or "1970-01-01T00:00:00.000Z")
        assert change["text"].get("cfi","").startswith("epubcfi(") or change["text"].get("page",0)>0
        if hasattr(self.server, 'save_entered'):
            self.server.save_entered.set()
            assert self.server.allow_save.wait(5), "fixture timed out during the pending save"
        self.server.statuses[200] = "read" if change["percentage"] >= 100 else "reading"
        self.server.progress[ident]=dict(fileId=ident,updatedAt="2026-11-07T10:00:00.000Z",percentage=change["percentage"],text=change["text"])
        self.server.saved.append(ident)
        return self.reply(200,{"results":[{"fileId":ident,"status":"saved"}]})

    def do_GET(self):
        if self.headers.get("Authorization") != f"Bearer {self.server.token}":
            return self.reply(401, {"statusCode": 401})
        url = urlsplit(self.path)
        query = parse_qs(url.query)
        page = int(query.get("page", [0])[0])
        size = int(query.get("size", [24])[0])
        def send_page(items):
            return self.reply(200, {"items":items[page*size:(page+1)*size], "total":len(items), "page":page, "size":size})
        if url.path in [API+"/books/200",API+"/books/201"]:
            idx=int(url.path.rsplit('/',1)[1]);b=self.server.card(idx-200)
            return self.reply(200,dict(title=b['title'],files=[dict(id=f["id"], contentVersion=getattr(self.server, "revision", "1")) for f in b["files"]],authors=[{"id":410,"name":a} for a in b["authors"]],description="<p>Letters from the northern lighthouse.</p>",publisher="Harbor Press",language="en",isbn13="9780000000002",seriesName="Beacon",seriesId=49,seriesIndex=2,libraryName="Port",readStatus={"status":self.server.statuses.get(idx,"unread"),"startedAt":None,"finishedAt":None}))
        if url.path.startswith(API + "/dashboard/scrollers/"):
            key = url.path.rsplit("/", 1)[1]
            indexes = {"continue-reading":[i-200 for i,st in self.server.statuses.items() if st in ("reading","rereading")], "recently-added":[0,1], "random":[0]}[key]
            items = [self.server.card(i) for i in indexes]
            return self.reply(200, items if key == "random" else {"books":items, "total":len(items)})
        for endpoint, total, target in [("authors",410,"Роджер Желязны"),("series",49,"Хроники Амбера")]:
            if url.path == f"{API}/{endpoint}":
                assert query["sort"] == ["name"] and query["order"] == ["asc"]
                groups = [{"id":n,"name":target if n==total else f"Group {n:04}", "bookCount":2}
                          for n in range(1,total+1)]
                q = query.get("q", [""])[0].casefold()
                return send_page([g for g in groups if q in g["name"].casefold()])
            if url.path == f"{API}/{endpoint}/{total}/books":
                assert query["sort"] == (["title"] if endpoint=="authors" else ["seriesIndex"])
                assert query["order"] == ["asc"] and (endpoint=="series" or query["collapseSeries"] == ["false"])
                return send_page([self.server.card(i) for i in ([0,1] if endpoint=="authors" else [1,0])])
        if url.path == API + "/books/201/sessions":
            page = int(query['page'][0])
            size = int(query['pageSize'][0])
            assert page >= 1 and size == 25
            assert query['sortBy'] == ['startedAt'] and query['sortDir'] == ['desc']
            total = 30 if self.server.statuses.get(201) != 'unread' else 0
            items = [dict(id=i, startedAt=f'2026-10-{i:02}T12:00:00.000Z', endedAt=f'2026-10-{i:02}T12:10:00.000Z',
                          durationSeconds=600, source='koreader', sessionType='read',
                          format='epub', endProgress=float(i)) for i in range(total, 0, -1)]
            return self.reply(200, dict(items=items[(page-1)*size:page*size], total=total, page=page, pageSize=size,
                                        stats=dict(totalSeconds=total*600)))
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
            if self.server.personal or "bookIds" in query:
                book = int(query.get("bookIds", ["0"])[0])
                items = [dict(id=i, name=n, bookCount=len(m), memberCount=int(book in m), isOwner=True, mediaType="books")
                         for i, (n, m) in self.server.personal.items()]
                if not book:
                    items.append(dict(id=150, name="Podcasts only", bookCount=0, isOwner=True, mediaType="podcasts"))
                return self.reply(200, items)
            return self.reply(200, [{"id": 91, "userId": 7, "name": COLLECTION, "bookCount": TOTAL}]
                              if self.server.has_collections else [])
        if url.path == API + "/collections/100/books":
            return send_page([self.server.card(i - 200) for i in sorted(self.server.personal[100][1])])
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
        if url.path == API + "/books/200/cover":
            return self.reply(200, ORIGINAL_COVER, "image/png")
        if url.path == API + "/books/200/thumbnail":
            return self.reply(200, (ROOT / "scripts/fixtures/shelf-cover.jpg").read_bytes(), "image/jpeg")
        if url.path == API + "/books/201/thumbnail":
            return self.reply(404, {"statusCode": 404})
        for ident, (_, mime, data) in self.server.files.items():
            if url.path == API + f"/books/files/{ident}/serve":
                tag = chr(34) + hashlib.sha256(data).hexdigest() + chr(34)
                if self.headers.get("If-None-Match") == tag:
                    return self.reply(304, b"", mime, etag=tag)
                return self.reply(200, data, mime, etag=tag)
        self.reply(404, {"path": self.path, "statusCode": 404})


def search_box(frame, heading):
    rows = frame.splitlines()
    for top, row in enumerate(rows):
        start = row.find("╭ " + heading)
        if start < 0:
            continue
        right = row.find("╮", start)
        for bottom in range(top + 1, len(rows)):
            if rows[bottom][start:start+1] == "╰" and rows[bottom][right:right+1] == "╯":
                return start, top, right + 1, bottom + 1
    return None


def search_body(frame, heading):
    bounds = search_box(frame, heading)
    if not bounds:
        return ""
    left, top, right, bottom = bounds
    return "\n".join(row[left + 1:right - 1] for row in frame.splitlines()[top + 1:bottom - 1])


class Terminal:
    sessions = itertools.count()

    def __init__(self, tmp, binary, url=None, args=None):
        self.base = ["tmux", "-f", "/dev/null", "-S", str(tmp / f"tmux-{next(self.sessions)}.sock")]
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
        # Deliver the complete SGR report in one paste, as a terminal would write it.
        # A synthetic, bytewise Esc under load can be read as the Escape key.
        self.run("set-buffer", "-b", "mouse", seq)
        paste = ["paste-buffer", "-d", "-r", "-b", "mouse", "-t", "shelf"]
        # Newer tmux sanitizes controls by default. Older tmux pastes them verbatim.
        if self.run(*paste, "-S", required=False).returncode:
            self.run(*paste)


def login(term, url, wrong_password=False):
    home = term.see("Connect to BookOrbit")
    assert "Your TUI book reader." in home and "EPUB  ·  FB2" in home
    term.keys("Home")
    term.see("Browse a folder on this device")
    term.keys("Down")
    term.see("Sign in to a BookOrbit server")
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
    term.wait("Account dashboard", lambda frame:"Discover something new" in frame and "Continue reading" in frame and "37%" in frame)
    term.keys("1")
    return term.wait("All books", lambda frame: BOOK in frame and OTHER_BOOK in frame)


def formats(term):
    return term.wait("Выбор EPUB / FB2 / PDF", lambda s: sum(
        1 for line in s.splitlines()
        if any(label in line for label in ("EPUB", "FB2", "PDF"))) >= 3)


def quit_shelf(term):
    if "Choose a library" not in term.run("capture-pane","-p","-t","shelf").stdout:
        term.keys("q");term.see("Choose a library")
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
        term.keys("/")
        frame = term.see("Search all books")
        # Both the search dialog and its original shelf remain on screen.
        assert "[V Grid]" in frame and frame.index("BookOrbit") < frame.index("Search all books"), frame
        term.click("[V Grid]", frame)
        term.wait("Outside click dismisses search without switching the shelf view",
                  lambda f: "Search all books" not in f and "[V Grid]" in f)
        term.keys("/");term.see("Search all books")
        term.type("9")
        term.wait("First character filters as you type", lambda f: "Архивный том 99" in search_body(f, "Search all books") and BOOK not in search_body(f, "Search all books"))
        term.keys("C-u")
        term.type("arhv 101")
        term.wait("Transliterated fuzzy result beyond page one", lambda f: "1 result" in f and "Архивный том 101" in f)
        term.type("xxx");term.see("No matches.")
        term.keys("BSpace","BSpace","BSpace");term.see("Архивный том 101")
        term.keys("Enter");term.see("Editions")
        term.keys("Escape");term.wait("Back restores the query", lambda f:"arhv 101" in f and "Архивный том 101" in f)
        term.keys("Escape");term.wait("Return to list", lambda f: "Search all books" not in f and "[V Grid]" in f)
        term.keys("2");term.see("37%");assert OTHER_BOOK in term.see("Currently reading")
        term.keys("3");term.see(f"Архивный том {TOTAL}")
        term.keys("1");term.see(BOOK)
        term.click("[V Grid]", term.see("[V Grid]"))
        term.see("▀")
        term.keys("Right","Enter")
        term.wait("Сетка: соседняя книга", lambda s: OTHER_BOOK in s and BOOK not in s)
        term.keys("Escape")
        term.see("[V List]")
        term.keys("5")
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
        term.wait("PDF opens directly from the edition row", lambda s: "PDF" in s and "1 / 3" in s)
        term.keys("Escape")
        for label, heading in [("FB2", FB2_MARK), ("EPUB", EPUB_MARK)]:
            term.click(label, formats(term))
            term.see(heading)
            if label == "FB2":
                term.keys("z");term.wait("Zen",lambda s:"Z Zen" not in s);term.keys("o");term.see("Chapters:");term.keys("End","Enter")
                term.wait("Конец книги FB2",lambda s:s.strip().startswith("Обратный рейс"))
                term.keys("z");term.see("Z Zen")
            term.keys("q")
            formats(term)
        term.keys("Escape")
        shelf = term.wait("Возврат к сетке", lambda s: BOOK in s and OTHER_BOOK in s)
        term.click(OTHER_BOOK, shelf)
        missing_cover = term.wait("Оттиск без обложки", lambda s: OTHER_BOOK in s and "PDF" in s and BOOK not in s)
        term.click("PDF", missing_cover)
        term.wait("PDF without cover opens from its edition row", lambda s: "PDF" in s and "1 / 3" in s)
        term.keys("Escape"); term.see("[D Save offline]")
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
        assert downloads == [f"{API}/books/files/{i}/serve" for i in (303, 302, 301, 4001)], requests
        assert [(path, status) for method, path, status in requests if method == "POST" and path.startswith(API + "/auth/")] == [
            (API + "/auth/login", 401), (API + "/auth/login", 200),
            (API + "/auth/refresh", 200), (API + "/auth/logout", 200)
        ], requests
        config = tomllib.loads((tmp / "config/botui/config.toml").read_text())["bookorbit"]
        assert {k:v for k,v in config.items() if not k.endswith("_collapsed")} == {"server": url, "username": USER, "list_view": False}, "Проверьте сохраняемые поля подключения"
    finally:
        term.run("kill-server", required=False)


def check_without_collections(tmp, binary, fixture):
    url = f"http://127.0.0.1:{fixture.server_port}/reader"
    term = Terminal(tmp, binary, url)
    try:
        all_books = login(term, url)
        term.run("resize-window","-t","shelf","-x","148","-y","38")
        names = ["0 Dashboard","1 All books","2 Currently reading","3 Recently added","4 Libraries","5 Collections","6 Authors","7 Series"]
        def totals(frame):
            return re.search(r"6 Authors +410",frame) and re.search(r"7 Series +49",frame)
        wide = term.wait("Totals before opening directories",totals)
        assert [wide.index(v) for v in names] == sorted(wide.index(v) for v in names)
        term.keys("0")
        dash = term.wait("Widgets and account shelves",lambda f:"Streak Builder" in f and "Continue reading" in f and "37%" in f)
        term.click("6 Authors",dash)
        term.see("Group 0001")
        term.keys("End")
        term.wait("Directory auto-pagination",lambda f:any(f"page=1&" in path and "/authors?" in path for _,path,_ in fixture.requests))
        term.keys("/")
        term.see("Search authors")
        term.type("zhelz");term.see("Роджер Желязны")
        term.keys("Enter")
        term.see(BOOK)
        term.keys("Enter")
        term.click("EPUB",formats(term));term.see(EPUB_MARK)
        term.keys("q");formats(term);term.keys("Escape");term.see(BOOK)
        term.keys("Escape");term.see("Роджер Желязны")
        term.keys("Escape");term.wait("Directory totals after live search", totals)
        term.keys("7");term.wait("Series directory", lambda f: "Series  ·  49" in f and "Group 0001" in f);term.keys("/");term.see("Search series")
        term.type("амб");frame = term.see("Хроники Амбера");term.click("Хроники Амбера", frame)
        term.see(OTHER_BOOK)
        term.keys("Enter")
        term.wait("First volume uses server sequence",lambda f:OTHER_BOOK in f and "[D Save offline]" in f)
        term.keys("1");term.see(BOOK)
        term.keys("Tab");term.see("37%")
        term.keys("Tab");term.see(OTHER_BOOK)
        term.keys("BTab");term.see("37%")
        term.keys("1");term.see(BOOK)
        # These two titles belong to different libraries. Collections on this account are empty.
        term.run("resize-window","-t","shelf","-x","44","-y","16")
        narrow = term.wait("Compact navigation uses the same order",lambda f: ("5 C" in f and "410" in f and "49" in f and "[V List]" in f))
        labels = ["1 All", "2 R", "3 +", "4 L", "5 C"]
        assert [narrow.index(v) for v in labels] == sorted(narrow.index(v) for v in labels)
        term.click("2 R",narrow)
        term.see("37%")
        term.keys("Tab")
        term.see(OTHER_BOOK)
        term.keys("1")
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
        term.keys("4")
        for library_id, expected, other in [(31, BOOK, OTHER_BOOK), (32, OTHER_BOOK, BOOK)]:
            listing = term.wait("Выбор библиотеки", lambda s: all(name in s for name in LIBRARIES.values()))
            term.click(LIBRARIES[library_id], listing)
            term.wait(f"Книги библиотеки {library_id}", lambda s: expected in s and other not in s)
            term.keys("Escape")
        term.wait("Возврат к библиотекам", lambda s: all(name in s for name in LIBRARIES.values()))
        term.keys("5")
        term.wait("Пустой список коллекций", lambda s: "collections" in s.lower()
                  and ("no personal" in s.lower()) and BOOK not in s)
        term.keys("Escape")
        term.wait("Книги доступны после выхода из пустых коллекций", lambda s: BOOK in s and OTHER_BOOK in s)
        term.keys("5"); term.see("[N New collection]"); term.keys("n")
        term.see("New collection name"); term.type("Winter reading"); term.keys("Enter")
        term.see("Winter reading")
        term.keys("1"); term.click(BOOK, term.see(BOOK))
        term.click("[A Collections]", term.see("[A Collections]"))
        term.click("Add to Winter reading", term.see("Add to Winter reading"))
        term.see("Remove from Winter reading")
        assert fixture.personal[100][1] == {200}
        term.keys("5"); term.see("[N New collection]"); term.keys("Enter")
        term.see(BOOK); term.keys("Enter"); formats(term); term.keys("a")
        term.click("Remove from Winter reading", term.see("Remove from Winter reading"))
        term.see("Add to Winter reading")
        term.keys("Escape"); formats(term); term.keys("Escape")
        term.wait("Collection is empty after removal", lambda f: "Winter reading  ·  0" in f and BOOK not in f)
        assert not fixture.personal[100][1]
        term.keys("1"); term.click(BOOK, term.see(BOOK)); formats(term); term.keys("a")
        term.click("Add to Winter reading", term.see("Add to Winter reading"))
        term.see("Remove from Winter reading"); term.keys("5")
        listing = term.see("[Del Delete collection]")
        assert "Podcasts only" not in listing
        term.click("[Del Delete collection]", listing)
        assert "Winter reading" in term.see("Delete collection?")
        term.keys("Enter"); term.see("[Del Delete collection]")
        assert fixture.personal[100][1] == {200}
        assert not any(m == "DELETE" and p == API + "/collections/100" for m, p, _ in fixture.requests)
        fixture.deny_collection_delete = True
        term.keys("DC"); term.see("Delete collection?"); term.keys("Right", "Enter")
        term.wait("Deletion refused", lambda f: "403" in f)
        assert fixture.personal[100][1] == {200}
        fixture.deny_collection_delete = False
        term.keys("DC")
        term.click("[ Delete collection ]", term.see("[ Delete collection ]"))
        term.see("No personal collections yet")
        assert not fixture.personal and ("DELETE", API + "/collections/100", 204) in fixture.requests
        term.keys("1"); term.click(BOOK, term.see(BOOK)); term.click("EPUB", formats(term))
        term.see(EPUB_MARK); term.keys("q"); formats(term)
        quit_shelf(term)
        requests = fixture.requests
        for path in ("/books/query", "/libraries/31/books", "/libraries/32/books"):
            assert ("POST", API + path, 200) in requests, requests
        for path in ("/libraries", "/collections", "/books/files/301/serve"):
            assert ("GET", API + path, 200) in requests, requests
        assert not any("/collections/91/" in path for _, path, _ in requests), requests
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
