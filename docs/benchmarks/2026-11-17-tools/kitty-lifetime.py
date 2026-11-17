"""Bounded repeat of root's real-PDF growth case, with raster round-trip check."""
import importlib.util, json, sys, time, os
from pathlib import Path
from PIL import Image, ImageChops

B = Path(os.environ['BOTUI_BENCH_DIR'])
spec = importlib.util.spec_from_file_location('bench', Path(__file__).with_name('native.py'))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
l = b.live
profile = 'kitty-lifetime-' + sys.argv[1]
l.BINS['baseline'] = sys.argv[2]
gate = b.setup(profile, 'baseline', b.books['lessons'])
rows = []
turns = []

def sample(step):
    record = b.sample(profile)
    record['step'] = step
    rows.append(record)
    print(json.dumps(record), flush=True)
    if record['total_rss_kib'] > 1300000:
        raise RuntimeError('Private test exceeded 1.3 GiB process-tree RSS')
    (B / (profile + '.json')).write_text(json.dumps({'memory': rows, 'turn_ms': turns}, indent=2))

def turn(key):
    before = l.text(profile)
    start = time.monotonic()
    l.key(profile, key)
    end = time.monotonic() + 12
    while time.monotonic() < end:
        frame = l.text(profile)
        if frame != before and '%' in frame:
            # This measures the terminal text/progress update, not first raster.
            turns.append(round((time.monotonic() - start) * 1000, 1))
            time.sleep(.2)
            return
        time.sleep(.08)
    raise AssertionError(('Page did not turn', key, before))

try:
    gate.touch()
    b.ready(profile, True)
    time.sleep(1)
    first = b.capture(profile, profile + '-opened')
    sample('opened')
    for cycle in range(5):
        for key in ['page_down'] * 8 + ['page_up'] * 8:
            turn(key)
        time.sleep(1)
        sample('cycle_' + str(cycle + 1))
    final = b.capture(profile, profile + '-final')
    # Exclude frame counters and window border; compare the actual PDF raster.
    crop = (150, 60, 1450, 900)
    diff = ImageChops.difference(Image.open(first).convert('RGB').crop(crop),
                                Image.open(final).convert('RGB').crop(crop))
    result = {'pixel_roundtrip_equal': diff.getbbox() is None, 'diff_bbox': diff.getbbox()}
    (B / (profile + '-roundtrip.json')).write_text(json.dumps(result, indent=2))
    print(json.dumps(result), flush=True)
finally:
    l.stop_pid(l.ROOT / (profile + '.pid'))
