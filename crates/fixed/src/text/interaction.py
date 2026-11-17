"""Optional precise PDF interaction; canonical text still comes from pdftotext."""
import sys
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr
import gi
gi.require_version("Poppler", "0.18")
from gi.repository import Poppler

doc = Poppler.Document.new_from_file(Path(sys.argv[1]).as_uri(), None)
page = doc.get_page(int(sys.argv[2]) - 1)
w, h = page.get_size()
print(f'<interaction width="{w}" height="{h}">')
text = page.get_text() or ""
ok, boxes = page.get_text_layout()
if ok and len(boxes) == len(text):
    for ch, r in zip(text, boxes):
        print(f'<char x0="{r.x1/w}" y0="{r.y1/h}" x1="{r.x2/w}" y1="{r.y2/h}">{escape(ch)}</char>')
for mapping in page.get_link_mapping():
    a, r = mapping.action, mapping.area
    # Link mappings use a bottom-left origin; text layouts use top-left.
    geometry = f'x0="{r.x1/w}" y0="{1-r.y2/h}" x1="{r.x2/w}" y1="{1-r.y1/h}"'
    if a.type == Poppler.ActionType.URI:
        print(f'<link {geometry} uri={quoteattr(a.uri.uri)}/>')
    elif a.type == Poppler.ActionType.GOTO_DEST:
        dest = a.goto_dest.dest
        if dest and dest.type == Poppler.DestType.NAMED:
            dest = doc.find_dest(dest.named_dest)
        if dest and 0 < dest.page_num <= doc.get_n_pages():
            target = doc.get_page(dest.page_num - 1)
            tw, th = target.get_size()
            x = dest.left / tw if dest.change_left else 0
            y = 1 - dest.top / th if dest.change_top else 0
            print(f'<link {geometry} page="{dest.page_num}" x="{x}" y="{y}"/>')
print('</interaction>')
