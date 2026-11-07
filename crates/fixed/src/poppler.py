# Fallback for Linux desktops which ship the Poppler GLib library but not poppler-utils.
import sys
from pathlib import Path
import gi
gi.require_version("Poppler","0.18")
from gi.repository import Poppler
import cairo
doc=Poppler.Document.new_from_file(Path(sys.argv[1]).as_uri(),None)
n=int(sys.argv[2])
if not n:
    print("Pages:",doc.get_n_pages())
else:
    page=doc.get_page(n-1);w,h=page.get_size();scale=int(sys.argv[3])/max(w,h)
    out=cairo.ImageSurface(cairo.FORMAT_RGB24,max(1,round(w*scale)),max(1,round(h*scale)))
    ctx=cairo.Context(out);ctx.set_source_rgb(1,1,1);ctx.paint();ctx.scale(scale,scale);page.render(ctx)
    out.write_to_png(sys.stdout.buffer)
