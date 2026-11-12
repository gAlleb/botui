#!/usr/bin/env python3
"""Fit margins must be available to zoom; zero-gap sheets form a spread."""
import importlib.util
import tempfile
import re
from pathlib import Path
spec = importlib.util.spec_from_file_location('scroll', Path(__file__).with_name('smoke-scroll.py'))
s = importlib.util.module_from_spec(spec); spec.loader.exec_module(s)
def widths(frame):
    return sorted([len(m[0]) for line in frame.splitlines() for m in re.finditer(r'[:#]{10,}', line)], reverse=True)
with tempfile.TemporaryDirectory(prefix='botui-page-layout-') as tmp:
    root = Path(tmp); book = root/'pages.pdf'; book.write_bytes(s.s.pdf(True))
    t = s.start(root, book)
    try:
        t.see('1 / 3'); t.keys('1', 'Space', 'r', '0')
        fit = t.wait('single fitted sheet', lambda f: widths(f) and '100%' in f)
        w = widths(fit)[0]
        t.keys('+', '+', '+')
        t.wait('zoom expands into side margins', lambda f: '150%' in f and widths(f) and widths(f)[0] > w)
        t.keys('Space', 'r', '0', '2', 'Space', 'l', 'w', '[', '[')
        t.see('Spread gap: 0 cells'); t.keys('Enter')
        joined = t.wait('joined spread', lambda f: '100%' in f and widths(f) and widths(f)[0] > w)
        joined_width = widths(joined)[0]
        t.keys('Space', 'l', 'w', ']', ']', ']', ']', 'Enter')
        t.wait('separated sheets', lambda f: widths(f) and widths(f)[0] < joined_width)
        s.stop(t)
        t = s.start(root, book); t.see('1 / 3'); t.keys('Space', 'l', 'w')
        t.see('Spread gap: 4 cells')
        print('Passed expanding page zoom, joined spreads, and persisted gap.')
    finally:
        t.run('kill-server', required=False)
