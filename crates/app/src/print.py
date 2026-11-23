import subprocess
import sys
import tempfile
from pathlib import Path


def printable(source, directory):
    kind = source.suffix.lower()
    markdown = kind in (".md", ".markdown")
    if kind == ".pdf":
        return source
    target = directory / "document.pdf"
    if kind in (".djvu", ".djv"):
        command = ["ddjvu", "-format=pdf", source, target]
    elif kind in (".epub", ".fb2", ".md", ".markdown"):
        reader = "markdown-raw_tex-raw_html-raw_attribute" if markdown else kind[1:]
        command = ["pandoc", "--sandbox", "--from", reader, "--pdf-engine=typst", "-V", "mainfont=DejaVu Serif",
                   "--output", target, source]
    else:
        raise ValueError(f"Unsupported print format: {kind}")
    subprocess.run(command, cwd=source.parent if markdown else directory, check=True, stdout=subprocess.DEVNULL)
    return target


def image_document(spool, count, target):
    import cairo
    from gi.repository import Gdk, GdkPixbuf
    if count < 1:
        raise ValueError("The document has no printable pages.")
    with cairo.PDFSurface(str(target), 1, 1) as surface:
        for index in range(count):
            page = GdkPixbuf.Pixbuf.new_from_file(str(spool / str(index)))
            width, height = page.get_width(), page.get_height()
            surface.set_size(width, height)
            canvas = cairo.Context(surface)
            Gdk.cairo_set_source_pixbuf(canvas, page, 0, 0)
            canvas.paint()
            canvas.show_page()


def print_document(source, spool=None, count=0):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Poppler", "0.18")
    from gi.repository import Gtk, Poppler
    if not Gtk.init_check()[0]:
        raise RuntimeError("Open a graphical desktop session to use the system print dialog.")
    with tempfile.TemporaryDirectory(prefix="botui-print-") as directory:
        if spool is None:
            pdf = printable(source, Path(directory))
        else:
            pdf = Path(directory) / "document.pdf"
            image_document(spool, count, pdf)
        document = Poppler.Document.new_from_file(pdf.as_uri(), None)
        operation = Gtk.PrintOperation()
        operation.set_job_name(source.name)
        operation.set_n_pages(document.get_n_pages())
        operation.set_unit(Gtk.Unit.POINTS)
        def draw_page(operation, context, index):
            page = document.get_page(index)
            width, height = page.get_size()
            scale = min(context.get_width() / width, context.get_height() / height)
            canvas = context.get_cairo_context()
            canvas.translate((context.get_width() - width * scale) / 2,
                             (context.get_height() - height * scale) / 2)
            canvas.scale(scale, scale)
            page.render_for_printing(canvas)
        operation.connect("draw-page", draw_page)
        operation.run(Gtk.PrintOperationAction.PRINT_DIALOG, None)


if __name__ == "__main__":
    try:
        if len(sys.argv) == 4:
            print_document(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]))
        else:
            print_document(Path(sys.argv[1]))
    except ImportError as error:
        sys.exit(f"Printing requires GTK 3, Python GI, Cairo and Poppler: {error}")
    except Exception as error:
        sys.exit(str(error))
