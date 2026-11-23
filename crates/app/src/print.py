import sys
from pathlib import Path


def print_pdf(source):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Poppler", "0.18")
    from gi.repository import Gtk, Poppler
    if not Gtk.init_check()[0]:
        raise RuntimeError("Open a graphical desktop session to use the system print dialog.")

    document = Poppler.Document.new_from_file(source.as_uri(), None)
    operation = Gtk.PrintOperation()
    operation.set_job_name(source.name)
    operation.set_n_pages(document.get_n_pages())
    operation.set_unit(Gtk.Unit.POINTS)

    def draw_page(_operation, context, index):
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
        print_pdf(Path(sys.argv[1]))
    except ImportError as error:
        sys.exit(f"Printing requires GTK 3, Python GI, Cairo and Poppler: {error}")
    except Exception as error:
        sys.exit(str(error))
