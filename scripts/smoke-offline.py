"""Download a native book, read it without a session, then reconnect to the original account."""
import importlib.util
import json
from pathlib import Path
import tempfile
import threading

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

with tempfile.TemporaryDirectory() as name, shelf.Fixture(False) as server:
    tmp = Path(name)
    binary = shelf.ROOT / "target/debug/botui"
    url = f"http://127.0.0.1:{server.server_port}/reader"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    term = shelf.Terminal(tmp, binary, url)
    try:
        shelf.login(term, url)
        term.keys("Enter")
        shelf.formats(term)
        term.keys("Down", "d")
        term.see("Saved in Offline downloads")
        shelf.quit_shelf(term)
        network = len(server.requests)
        term = shelf.Terminal(tmp, binary, args=[])
        term.click("Offline downloads", term.see("Offline downloads"))
        term.click(shelf.BOOK, term.see(shelf.BOOK))
        term.see(shelf.FB2_MARK)
        term.keys("o")
        term.wait("Contents overlay", lambda f:"[ Back ]" in f and "Chapters:" in f)
        term.keys("End", "Enter", "q")
        term.see("Local library")
        term.keys("q")
        term.see("Offline downloads")
        shelf.quit_shelf(term)
        assert network == len(server.requests)
        meta, = (tmp/".local/share/botui/downloads").glob(".*.json")
        record = json.loads(meta.read_text())
        assert record["pending"]["target"]["Text"].startswith("epubcfi(")
        term = shelf.Terminal(tmp, binary, url)
        shelf.login(term, url)
        assert server.saved == [302]
        assert server.progress[302]["text"]["cfi"] == record["pending"]["target"]["Text"]
        assert json.loads(meta.read_text())["pending"] is None
        shelf.quit_shelf(term)

        # The last screen can finish at the very same CFI as the saved position.
        completed = json.loads(meta.read_text())
        completed["base"]["percentage"] = 99
        server.progress[302]["percentage"] = 99
        completed["pending"] = {"target": record["pending"]["target"], "percentage": 100}
        meta.write_text(json.dumps(completed))
        term = shelf.Terminal(tmp, binary, url)
        shelf.login(term, url)
        assert server.saved == [302, 302] and server.progress[302]["percentage"] == 100
        assert json.loads(meta.read_text())["pending"] is None
        shelf.quit_shelf(term)
    finally:
        term.run("kill-server", required=False)
        server.shutdown()
print("Passed offline reading, conditional upload and finishing at an unchanged location.")
