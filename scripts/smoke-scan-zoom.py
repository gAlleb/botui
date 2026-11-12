#!/usr/bin/env python3
"""Hold PDF refinement in a worker; zoom must keep the previous raster visible.

Runs in private tmux sessions. An optional executable permits checking an older
release with exactly the same input, without touching a real reading position.
"""
import importlib.util
import os
from pathlib import Path
import re
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("scroll", ROOT / "scripts/smoke-scroll.py")
scroll = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scroll)


def check(binary, directory, mode):
    directory.mkdir()
    book = directory / "zoom.pdf"
    book.write_bytes(scroll.s.pdf(True, shades=(.3,) * 6))
    config = directory / "config/botui"
    config.mkdir(parents=True)
    config.joinpath("config.toml").write_text(f'reading_layout="{mode}"\nzen=false\n')
    bins = directory / "bin"
    bins.mkdir()
    renderer = bins / "pdftoppm"
    renderer.write_text('''#!/usr/bin/env python3
import sys,time
from pathlib import Path
d=Path(__file__).parent
n=int(sys.argv[sys.argv.index('-f')+1])
side=int(sys.argv[sys.argv.index('-scale-to')+1])
try:
    with (d/'base').open('x') as f: f.write(str(side))
except FileExistsError: pass
with (d/'calls').open('a') as f: print(n,side,file=f)
base=int((d/'base').read_text())
if side>base:
    while not (d/f'release-{side}').exists(): time.sleep(.01)
value=76 if side==base else (205 if side>=base*2 else 173)
sys.stdout.buffer.write(b'P6\\n20 30\\n255\\n'+bytes([value])*1800)
sys.stdout.buffer.flush()
(d/f'done-{n}-{side}').touch()
''')
    renderer.chmod(0o755)
    term = scroll.s.shelf.Terminal(directory, Path("/usr/bin/env"), args=[
        "NO_COLOR=1", "PATH=" + str(bins) + os.pathsep + os.environ["PATH"],
        str(binary), str(book)])

    def calls():
        return [tuple(map(int, line.split())) for line in bins.joinpath("calls").read_text().splitlines()]

    def ready(side):
        visible = 2 if mode == "spread" else 1
        radius = 2 if side > 1600 else 3
        return all(bins.joinpath(f"done-{n}-{side}").exists()
                   for n in range(1, min(6, visible + radius) + 1))

    try:
        term.see("::::::::")
        base = int(bins.joinpath("base").read_text())
        # Each level changes raster resolution before it reaches the 1920 px cap.
        # The renderer is deliberately held, so this checks the intermediate
        # frame rather than merely waiting until the flicker has gone away.
        for percent in (110, 125, 150, 175, 200):
            side = min(1920, base * percent // 100)
            start = time.monotonic()
            term.keys('Space', 'r', '+')
            term.wait(f"{mode}: zoom {percent}% retains the readable page", lambda f:
                f"[Space l w {percent}%]" in f and "Rendering page" not in f
                and re.search(r"[:*]{12,}", f))
            assert time.monotonic() - start < .5, "Zoom waited for the PDF worker"
            term.wait("Larger raster requested", lambda _: (1, side) in calls())
            assert not bins.joinpath(f"done-1-{side}").exists(), "Refinement was not held"
            bins.joinpath(f"release-{side}").touch()
            marker = "########" if percent == 200 else "********"
            term.wait("Refined page replaces the preview", lambda f:
                marker in f and (percent != 200 or "********" not in f) and ready(side))
        # Speculative neighbors may be cancelled as the prefetch radius changes.
        # The pages being read must not start another render at a cached size.
        visible = 2 if mode == "spread" else 1
        visible_calls = lambda: [call for call in calls() if call[0] <= visible]
        rendered = visible_calls()
        term.keys('Space', 'r', '-', 'Space', 'r', '-', 'Space', 'r', '-', 'Space', 'r', '-', 'Space', 'r', '-')
        term.wait("Smaller sizes reuse the best raster", lambda f:
            "[Space l w 100%]" in f and "########" in f and "Rendering page" not in f)
        assert visible_calls() == rendered, (mode, "Downscaling re-rendered PDF", rendered, calls())
        # Zooming in again uses the same higher-quality cache entry as well.
        term.keys('Space', 'r', '+', 'Space', 'r', '+', 'Space', 'r', '+', 'Space', 'r', '+', 'Space', 'r', '+')
        term.see("[Space l w 200%]")
        assert visible_calls() == rendered, (mode, "Repeated zoom re-rendered PDF", rendered, calls())
        print(f"{mode}: pending refinements remain visible; smaller and repeated zooms reuse rasters", flush=True)
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/botui"
    with tempfile.TemporaryDirectory(prefix="botui-scan-zoom-") as temp:
        for mode in ("single", "spread", "scroll"):
            check(binary, Path(temp) / mode, mode)
