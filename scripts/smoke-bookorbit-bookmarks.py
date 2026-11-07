"""Delete a selected BookOrbit bookmark without an edition address, including stale and denied requests."""
import importlib.util
from pathlib import Path
import tempfile
import threading

path = Path(__file__).with_name("smoke-shelf.py")
spec = importlib.util.spec_from_file_location("shelf", path)
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)


def open_book(tmp, server):
    url = f"http://127.0.0.1:{server.server_port}/reader"
    t = s.Terminal(tmp, s.ROOT / "target/debug/botui", url)
    s.login(t, url)
    t.keys("Enter")
    t.click("FB2", s.formats(t))
    t.see(s.FB2_MARK)
    t.keys("g")
    return t


def close_book(t):
    t.keys("Escape")
    t.wait("Close bookmarks", lambda f: "Bookmarks:" not in f and s.FB2_MARK in f)
    t.keys("q")
    s.formats(t)
    s.quit_shelf(t)


def confirm(t):
    t.see("Delete bookmark?")
    t.click("[ Delete bookmark ]", t.see("[ Delete bookmark ]"))


def exercise(tmp, server):
    # No anchor can identify these rows. The title also isn't a unique key.
    server.marks[302] = [
        {"id": i, "title": name, "cfi": None}
        for i, name in [(11,"Twin"), (12,"Twin"), (14,"Other device"), (15,"Keep on failure")]
    ]
    server.denied_marks = {15}
    ids = lambda: [m["id"] for m in server.marks[302]]
    t = open_book(tmp, server)
    try:
        t.see("Bookmarks: 4")
        t.keys("Down", "Delete")
        t.see("Delete bookmark?")
        t.keys("Enter")  # default is Cancel
        t.see("Bookmarks: 4")
        assert ids() == [11, 12, 14, 15] and not any(v[0] == "DELETE" for v in server.requests)

        t.keys("Delete")
        confirm(t)
        t.see("Bookmarks: 3")
        assert ids() == [11, 14, 15]
        assert any(v[0] == "DELETE" and v[1].endswith("/200/bookmarks/12") for v in server.requests)
        close_book(t)
        t = open_book(tmp, server)
        t.see("Bookmarks: 3")

        # Another device removed the selected row while the list was open.
        server.marks[302].pop(1)
        t.keys("Down", "Delete")
        confirm(t)
        t.see("Bookmarks: 2")
        assert ids() == [11, 15]
        # Clicking × in a different row must act on that row, not on the selection.
        t.keys("Home")
        frame = t.see("Keep on failure")
        target = "\n".join(line if "Keep on failure" in line else "" for line in frame.splitlines())
        t.click("[×]", target)
        confirm(t)
        t.see("403")
        t.keys("Escape")
        t.see("Bookmarks: 2")
        t.see("Keep on failure")
        assert ids() == [11, 15]
    finally:
        t.run("kill-server",required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as tmp, s.Fixture(False) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            exercise(Path(tmp), server)
        finally:
            server.shutdown()
            thread.join()
    print("Passed selected server bookmark deletion, cancellation, restart, concurrent removal and denied deletion.")
