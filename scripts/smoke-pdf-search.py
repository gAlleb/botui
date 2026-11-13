#!/usr/bin/env python3
"""PDF occurrence navigation, raster highlights, offscreen reveal and Vim cursor."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
BINARY = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui')).resolve()


def pdf():
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Count 3 /Kids [3 0 R 5 0 R 7 0 R] >>']
    contents = [b'BT /F1 18 Tf 30 740 Td (lighthouse alpha lighthouse bravo) Tj 0 -600 Td (lighthouse charlie) Tj ET', b'', b'BT /F1 18 Tf 30 300 Td (lighthouse delta) Tj ET']
    for i, content in enumerate(contents):
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {4 + i * 2} 0 R >>'.encode())
        objects.append(f'<< /Length {len(content)} >>\nstream\n'.encode() + content + b'\nendstream')
    data = bytearray(b'%PDF-1.4\n'); offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode() + obj + b'\nendobj\n')
    offset = len(data); data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for pos in offsets[1:]:
        data.extend(f'{pos:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode())
    return data


def check(root, mode, fallback=False, zoom=100, rotate=False):
    d = root / (mode + ('-fallback' if fallback else '') + f'-{zoom}-{rotate}')
    (d / 'config/botui').mkdir(parents=True)
    (d / 'config/botui/config.toml').write_text(f'reading_layout = "{mode}"\nscan_zoom = {zoom}\n')
    book = d / 'search.pdf'; book.write_bytes(pdf())
    helpers = d / 'bin'; helpers.mkdir()
    for name in ['wl-copy', 'xclip', 'xsel', 'pbcopy']:
        p = helpers / name; p.write_text('#!/bin/sh\n/bin/cat > "$HOME/copied"\n'); p.chmod(0o755)
    if fallback:
        for name in ['python3']:
            (helpers / name).symlink_to(shutil.which(name))
    path = str(helpers) + ('' if fallback else os.pathsep + os.environ['PATH'])
    t = s.Terminal(d, Path('/usr/bin/env'), args=['TERM=xterm', 'COLORTERM=', 'PATH=' + path, str(BINARY), str(book)])
    def raster():
        return '\n'.join(row for row in t.run('capture-pane', '-p', '-e', '-t', 'shelf').stdout.splitlines() if '▀' in row or '▄' in row)
    def match(i):
        t.see(f'Match {i} / 4')
        t.wait('Rendered found word', lambda f: ('▀' in f or '▄' in f) and 'Rendering' not in f)
    def copied(expected):
        for _ in range(150):
            if (d / 'copied').exists() and (d / 'copied').read_text() == expected:
                return
            time.sleep(.025)
        raise AssertionError((expected, (d / 'copied').read_text() if (d / 'copied').exists() else 'nothing copied'))
    try:
        t.see('1 / 3')
        if rotate:
            t.keys('Space', 'r', 'r')
        t.keys('Space', 'f'); t.see('Find in text layer'); t.type('lighthouse'); t.keys('Enter'); match(1)
        first = raster()
        t.keys('F3'); match(2); second = raster()
        assert first != second, 'Same-page matches did not change the raster highlight'
        t.keys('F4'); match(1)
        assert first == raster(), 'Previous occurrence did not restore its position and highlight'
        t.keys('F3'); match(2)
        t.keys('F3'); match(3)
        assert raster() != second
        t.keys('F3'); match(4); t.see('3 / 3')
        t.keys('F3'); match(1); t.see('1 / 3')
        t.keys('F5'); t.see('Search results'); t.keys('End', 'Enter'); match(4); t.see('3 / 3')
        t.keys('Space', 'c'); t.see('NORMAL')
        t.keys('n'); match(1)
        t.keys('w', 'y', 'i', 'w'); copied('alpha')
        t.keys('b', 'n', 'w', 'y', 'i', 'w'); copied('bravo')
        t.keys('b', 'N', 'w', 'y', 'i', 'w'); copied('alpha')
        t.keys('b', 'n', 'n', 'w', 'y', 'i', 'w'); copied('charlie')
        # Manual movement past the middle hit changes the origin of the next search.
        t.keys('b', 'N', 'w', 'n', 'w', 'y', 'i', 'w'); copied('charlie')
        t.keys('Escape'); t.keys('Space', 'f'); t.see('Find in text layer'); t.keys('C-u'); t.type('absent-token'); t.keys('Enter'); t.see('No matches')
        print('PASS PDF', mode, 'GI' if fallback else 'pdftotext', zoom, 'rotated' if rotate else '', flush=True)
    finally:
        t.run('kill-server', required=False)


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='botui-pdf-search-') as tmp:
        for mode in ['single', 'spread', 'scroll']:
            check(Path(tmp), mode)
        gi = subprocess.run([sys.executable, '-I', '-c', "import gi, cairo; gi.require_version('Poppler', '0.18'); from gi.repository import Poppler"], capture_output=True)
        if gi.returncode == 0:
            check(Path(tmp), 'scroll', fallback=True)
        else:
            print('SKIP optional Poppler GI fallback: bindings unavailable')
        for mode in ['single', 'spread', 'scroll']:
            check(Path(tmp), mode, zoom=160, rotate=True)
