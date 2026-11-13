# Fallback for Linux desktops which ship the Poppler GLib library but not poppler-utils.
import sys
from pathlib import Path
import gi
gi.require_version("Poppler","0.18")
from gi.repository import Poppler
import cairo
doc=Poppler.Document.new_from_file(Path(sys.argv[1]).as_uri(),None)
if sys.argv[2] == "text":
    for index in range(doc.get_n_pages()):
        sys.stdout.write((doc.get_page(index).get_text() or "") + "\f")
    sys.exit(0)
if sys.argv[2] == "text-boxes":
    import re
    from xml.sax.saxutils import escape
    pages = [int(sys.argv[3]) - 1] if len(sys.argv) > 3 else range(doc.get_n_pages())
    print("<doc>")
    for index in pages:
        page = doc.get_page(index)
        width, height = page.get_size()
        text = page.get_text() or ""
        ok, boxes = page.get_text_layout()
        if text.strip() and (not ok or len(boxes) != len(text)):
            raise RuntimeError("PDF text coordinates do not match its characters")
        print(f'<page width="{width}" height="{height}"><flow><block><line>')
        previous = 0
        for word in re.finditer(r"\S+", text):
            if "\n" in text[previous:word.start()]:
                print("</line><line>")
            rects = boxes[word.start():word.end()]
            x0, y0 = min(r.x1 for r in rects), min(r.y1 for r in rects)
            x1, y1 = max(r.x2 for r in rects), max(r.y2 for r in rects)
            print(f'<word xMin="{x0}" yMin="{y0}" xMax="{x1}" yMax="{y1}">{escape(word.group())}</word>')
            previous = word.end()
        print("</line></block></flow></page>")
    print("</doc>")
    sys.exit(0)
def render(n, side):
    page = doc.get_page(n - 1)
    w, h = page.get_size()
    scale = side / max(w, h)
    out = cairo.ImageSurface(cairo.FORMAT_RGB24, max(1, round(w*scale)), max(1, round(h*scale)))
    ctx = cairo.Context(out)
    ctx.set_source_rgb(1, 1, 1)
    ctx.paint()
    ctx.scale(scale, scale)
    page.render(ctx)
    out.flush()
    return out

if sys.argv[2] == "stream":
    import struct
    wire = sys.stdout.buffer
    for line in sys.stdin.buffer:
        n, side = map(int, line.split())
        surface = render(n, side)
        wire.write(struct.pack(">II", surface.get_width(), surface.get_height()))
        wire.write(surface.get_data())
        wire.flush()
elif int(sys.argv[2]):
    render(int(sys.argv[2]), int(sys.argv[3])).write_to_png(sys.stdout.buffer)
else:
    print("Pages:", doc.get_n_pages())
