#!/usr/bin/env python3
"""Follow an EPUB link, view its image and supplementary note; read an FB2 note and illustration."""
from base64 import b64encode
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", ROOT / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = ROOT / "target/debug/botui"

def select(t, label):
    t.keys("i")
    t.see("Book links & illustrations")
    t.click(label, t.see(label))

def check(tmp):
    pic = (ROOT / "scripts/fixtures/shelf-cover.jpg").read_bytes()
    epub = tmp / "Atlas.epub"
    with ZipFile(epub, "w") as z:
        z.writestr("META-INF/container.xml", '<container><rootfile full-path="a.opf"/></container>')
        z.writestr("a.opf", '<package><metadata><title>Atlas</title></metadata><manifest><item id="a" href="chap.xhtml" media-type="application/xhtml+xml"/><item id="n" href="notes.xhtml" media-type="application/xhtml+xml"/><item id="p" href="map.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="a"/><itemref idref="n" linear="no"/></spine></package>')
        z.writestr("chap.xhtml", '<html><body><h1>Harbour</h1><p>See <a href="#end">Next chapter</a>, <a href="notes.xhtml#memo">Source note</a> and <a href="https://example.org/map">Publisher website</a>.</p><p><img src="map.jpg" alt="Map plate"/></p>' + '<p>Walking along the bay.</p>' * 100 + '<h1 id="end">Observatory</h1><p>Last passage.</p></body></html>')
        z.writestr("notes.xhtml", '<html><body><p id="memo">In 1812 the harbour moved.</p></body></html>')
        z.writestr("map.jpg", pic)
    t = shelf.Terminal(tmp, BINARY, args=[str(epub)])
    try:
        t.see("Harbour")
        select(t, "Source note")
        t.see("In 1812 the harbour moved.")
        t.keys("Escape")
        t.wait("closed note", lambda f: "In 1812" not in f)
        select(t,"Map plate")
        t.wait("rendered embedded JPEG",lambda f:"[ Back · Esc ]" in f and ("▀" in f or "▄" in f))
        t.keys("Escape")
        t.wait("closed illustration", lambda f:"[ Back · Esc ]" not in f)
        select(t,"Publisher website")
        t.see("Open in browser: https://example.org/map")
        t.keys("Escape")
        t.wait("cancelled external link",lambda f:"External website" not in f)
        select(t, "Next chapter")
        t.wait("exact in-book destination",lambda f:"Last passage." in f and "Book links" not in f)
        t.keys(",")
        t.wait("back at paragraph with links",lambda f:"Publisher website" in f)
    finally:
        t.run("kill-server", required=False)
    fb2 = tmp / "Notes.fb2"
    fb2.write_text('<FictionBook xmlns:xlink="http://www.w3.org/1999/xlink"><body><section><title><p>Evening tide</p></title><p>Read <a xlink:href="#n">Footnote one</a>.</p><image xlink:href="#chart"/></section></body><body name="notes"><section id="n"><p>The tide was high.</p></section></body><binary id="chart" content-type="image/jpeg">'+b64encode(pic).decode()+'</binary></FictionBook>')
    t = shelf.Terminal(tmp, BINARY, args=[str(fb2)])
    try:
        start = t.see("Evening tide")
        assert "The tide was high." not in start
        select(t,"Footnote one");t.see("The tide was high.")
        t.keys("Escape");t.wait("closed note",lambda f:"The tide was high." not in f)
        select(t,"Illustration · chart");t.wait("FB2 illustration",lambda f:"[ Back · Esc ]" in f and ("▀" in f or "▄" in f))
    finally:
        t.run("kill-server", required=False)

if __name__ == "__main__":
    with TemporaryDirectory(prefix="botui-resources-") as folder:
        check(Path(folder))
    print("Passed links, jump back, non-spine EPUB note, FB2 footnote and actual embedded image rendering.")
