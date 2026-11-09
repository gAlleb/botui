#!/usr/bin/env python3
"""Follow an EPUB link, view its image and supplementary note; read an FB2 note and illustration."""
from base64 import b64encode
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
import time
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
        z.writestr("chap.xhtml", '<html><body><h1>Harbour</h1><p>See <a href="#end">Next chapter</a>, <a id="source-note" href="notes.xhtml#memo">Source note</a> and <a href="https://example.org/map">Publisher website</a>.</p><p><img src="map.jpg" alt="Map plate"/></p><table><tr><th>Port</th><th>Tide</th></tr><tr><td>Whitby</td><td>High</td></tr></table>' + '<p>Walking along the bay.</p>' * 100 + '<h1 id="end">Observatory</h1><p>Last passage.</p></body></html>')
        z.writestr("notes.xhtml", '<html><body><aside id="memo"><p>In 1812 the harbour moved.</p></aside><p><a href="chap.xhtml#source-note" role="doc-backlink">Return to citation</a></p><aside id="other">Content of adjacent note</aside></body></html>')
        z.writestr("map.jpg", pic)
    t = shelf.Terminal(tmp, BINARY, args=[str(epub)])
    try:
        page=t.wait("inline illustration in reading flow",lambda f:"Harbour" in f and ("▀" in f or "▄" in f) and "Whitby" in f)
        assert any("Whitby" in line and "│" in line and "High" in line for line in page.splitlines()),page
        select(t, "Source note")
        note=t.see("In 1812 the harbour moved.")
        assert "Content of adjacent note" not in note
        t.click("[ Return to citation ]",t.see("[ Return to citation ]"))
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
        t.wait("back at paragraph with links", lambda f: "See Next chapter" in f and "Source note" in f)
    finally:
        t.run("kill-server", required=False)
    fb2 = tmp / "Notes.fb2"
    fb2.write_text('<FictionBook xmlns:xlink="http://www.w3.org/1999/xlink"><body><section><title><p>Evening tide</p></title><p>Read <a xlink:href="#n">Footnote one</a>.</p><image xlink:href="#chart"/></section></body><body name="notes"><section id="n"><p>The tide was high.</p></section></body><binary id="chart" content-type="image/jpeg">'+b64encode(pic).decode()+'</binary></FictionBook>')
    t = shelf.Terminal(tmp, BINARY, args=[str(fb2)])
    try:
        # A direct-file reader negotiates graphics when the first image arrives.
        # Wait for that response/timeout before sending interactive test keys.
        start = t.wait("FB2 inline illustration ready", lambda f: "Evening tide" in f and ("▀" in f or "▄" in f))
        assert "The tide was high." not in start
        select(t,"Footnote one");t.see("The tide was high.")
        t.keys("Escape");t.wait("closed note",lambda f:"The tide was high." not in f)
        select(t,"Illustration · chart");t.wait("FB2 illustration",lambda f:"[ Back · Esc ]" in f and ("▀" in f or "▄" in f))
    finally:
        t.run("kill-server", required=False)

def check_reference_return(tmp):
    import json
    import time
    book = tmp / "Glossary.epub"
    with ZipFile(book, "w") as z:
        z.writestr("META-INF/container.xml", '<container><rootfile full-path="a.opf"/></container>')
        z.writestr("a.opf", '<package><metadata><title>Tide tables</title></metadata><manifest>' +
                   ''.join(f'<item id="{n}" href="{n}.xhtml" media-type="application/xhtml+xml"/>' for n in ('text','glossary','extra')) +
                   '</manifest><spine><itemref idref="text"/><itemref idref="glossary"/></spine></package>')
        z.writestr("text.xhtml", '<html><body>' + ''.join(f'<p>Coast waypoint {i:03}.</p>' for i in range(40)) +
                   '<p>Return marker: coast and <a id="ref" href="glossary.xhtml#term">ebb tide</a>; see also ' +
                   '<a href="extra.xhtml#datum">tidal datum</a>.</p>' +
                   ''.join(f'<p>Other waypoint {i:03}.</p>' for i in range(70)) + '</body></html>')
        z.writestr("glossary.xhtml", '<html><body><h1>Glossary</h1><p id="term">Water moves seaward.</p>' +
                   '<p>Return to the <a href="text.xhtml#ref" role="doc-backlink">original citation</a>.</p></body></html>')
        z.writestr("extra.xhtml", '<html><body><dl><dt id="datum">Datum</dt>' +
                   '<dd>Height relative to the tide gauge. <a role="doc-backlink" href="text.xhtml#ref">↩</a></dd>' +
                   '<dt>Next unrelated entry</dt><dd>Wrong definition.</dd></dl></body></html>')
    t=shelf.Terminal(tmp,BINARY,args=[str(book)])
    try:
        t.see("Coast waypoint 000")
        t.keys("3", "/"); t.see("Find:"); t.type("Coast waypoint 038"); t.see("1 match"); t.keys("Enter")
        source=t.wait("Reading before the cited paragraph",lambda f:"Find:" not in f and "Coast waypoint 038" in f and "Return marker" in f)
        t.click("ebb tide",source)
        note=t.see("Water moves seaward.")
        assert "[ Back · Esc ]" in note
        t.click("original citation",note)
        t.wait("Publisher backlink restores the exact viewport",lambda f:f==source)
        t.keys("F6");links=t.see("Links on this page")
        assert "tidal datum" in links and "original citation" not in links,links
        t.click("tidal datum",links)
        definition=t.see("Height relative to the tide gauge.")
        assert "Wrong definition" not in definition and "↩" in definition
        t.click("[ Back · Esc ]",definition)
        t.wait("Non-spine glossary keeps both the term and its definition",lambda f:f==source)
        t.keys("2");spread=t.see("ebb tide"); t.click("ebb tide",spread)
        t.see("Water moves seaward.")
        t.run("resize-window","-t","shelf","-x","155","-y","31")
        t.click("[ Back · Esc ]",t.see("[ Back · Esc ]"))
        t.wait("Spread reflows around original reading point",lambda f:"Coast waypoint 038" in f and "Return marker" in f and "Read here" not in f)
        t.keys("3"); t.click("ebb tide",t.see("ebb tide")); t.see("Water moves seaward.")
        t.keys("PageDown"); time.sleep(.8); t.keys("C-c")
        deadline=time.monotonic()+5
        while t.run("has-session","-t","shelf",required=False).returncode==0 and time.monotonic()<deadline:
            time.sleep(.05)
        records=[json.loads(f.read_text()) for f in (tmp/'state').rglob('*.json')]
        assert any(r.get("block")==38 and not r.get("finished") for r in records),records
        t.run("kill-server",required=False)
        t=shelf.Terminal(tmp,BINARY,args=[str(book)])
        t.wait("Closing from a footnote resumes reading at the source",lambda f:"Coast waypoint 038" in f and "Water moves seaward." not in f)
        t.click("ebb tide",t.see("ebb tide"));t.see("Water moves seaward.")
        t.keys("Enter");t.wait("Explicitly continue from the destination",lambda f:"Water moves seaward." in f and "Read here" not in f)
    finally:
        t.run("kill-server",required=False)


def check_scanned_epub(tmp):
    book = tmp / "Scans.epub"
    with ZipFile(book, "w") as z:
        z.writestr("META-INF/container.xml", '<container><rootfile full-path="a.opf"/></container>')
        z.writestr("a.opf", '<package><manifest><item id="body" href="pages.xhtml" media-type="application/xhtml+xml"/>'
                   '<item id="scan" href="scan.jpg" media-type="image/jpeg"/></manifest>'
                   '<spine><itemref idref="body"/></spine></package>')
        z.writestr("pages.xhtml", "<html><body>" + '<p><img src="scan.jpg"/></p>' * 12 + "</body></html>")
        z.write(ROOT / "scripts/fixtures/shelf-cover.jpg", "scan.jpg")
    config = tmp / "config/botui"
    config.mkdir(parents=True)
    (config / "config.toml").write_text('zen = false\nreading_layout = "single"\n')
    t = shelf.Terminal(tmp, BINARY, args=[str(book)])
    try:
        t.wait("first scanned page", lambda f: "1 / 12" in f and "▀" in f)
        t.keys("Right")
        t.wait("second scanned page", lambda f: "2 / 12" in f and "▀" in f)
        t.keys("2", "Right")
        t.wait("next scanned spread", lambda f: "4 / 12" in f and "▀" in f)
        t.keys("q")
        deadline = time.monotonic() + 5
        while t.run("has-session", "-t", "shelf", required=False).returncode == 0:
            if time.monotonic() >= deadline:
                raise AssertionError("Scanned reader did not close")
            time.sleep(.05)
    finally:
        t.run("kill-server", required=False)


if __name__ == "__main__":
    with TemporaryDirectory(prefix="botui-scans-") as folder:
        check_scanned_epub(Path(folder))
    with TemporaryDirectory(prefix="botui-resources-") as folder:
        check(Path(folder))
    with TemporaryDirectory(prefix="botui-reference-return-") as folder:
        check_reference_return(Path(folder))
    print("Passed on-page links, publisher backlinks, glossary definitions, exact reference return through resize and reopen, jump back, non-spine EPUB note, FB2 footnote and actual embedded image rendering.")
