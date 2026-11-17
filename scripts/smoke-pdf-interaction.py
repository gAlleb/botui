#!/usr/bin/env python3
"""Exact PDF substrings, link destinations and graceful optional-GI fallback."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
BINARY = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui')).resolve()


def pdf(rotation=0):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Count 3 /Kids [3 0 R 5 0 R 7 0 R] >>']
    contents = [
        b'BT /F1 16 Tf 35 445 Td (PDF interaction audit) Tj /F1 12 Tf 0 -40 Td (Alpha interoperability Omega) Tj 0 -25 Td (Second line: microarchitecture.) Tj 0 -80 Td (Go to page three) Tj 0 -35 Td (Open example.invalid) Tj ET',
        b'BT /F1 16 Tf 35 445 Td (Page two - intermediate) Tj ET',
        b'BT /F1 16 Tf 35 445 Td (Page three - target) Tj 0 -45 Td (INTERNAL-LINK-TARGET) Tj ET',
    ]
    for i, content in enumerate(contents):
        annotations = ' /Annots [9 0 R 10 0 R]' if i == 0 else ''
        turn = f' /Rotate {rotation}' if i == 0 else ''
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 500]{turn} /Resources << /Font << /F1 11 0 R >> >> /Contents {4+i*2} 0 R{annotations} >>'.encode())
        objects.append(f'<< /Length {len(content)} >>\nstream\n'.encode() + content + b'\nendstream')
    objects.extend([
        b'<< /Type /Annot /Subtype /Link /Rect [35 295 133 310] /Border [0 0 1] /A << /S /GoTo /D [7 0 R /XYZ 0 420 null] >> >>',
        b'<< /Type /Annot /Subtype /Link /Rect [35 260 159 275] /Border [0 0 1] /A << /S /URI /URI (https://example.invalid/botui-audit) >> >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    ])
    data = bytearray(b'%PDF-1.4\n'); offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode() + obj + b'\nendobj\n')
    offset = len(data); data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for pos in offsets[1:]: data.extend(f'{pos:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode())
    return data


def check(root, precise, rotation=0):
    d = root / (('precise' if precise else 'word-fallback')+f'-{rotation}')
    (d/'config/botui').mkdir(parents=True)
    (d/'config/botui/config.toml').write_text('reading_layout = "single"\n')
    book = d/'interaction.pdf'; book.write_bytes(pdf(rotation))
    helpers = d/'bin'; helpers.mkdir()
    for name in ['wl-copy', 'xclip', 'xsel', 'pbcopy']:
        p = helpers/name; p.write_text('#!/bin/sh\n/bin/cat > "$HOME/copied"\n'); p.chmod(0o755)
    p = helpers/'xdg-open'; p.write_text('#!/bin/sh\nprintf "%s" "$1" > "$HOME/opened-url"\n'); p.chmod(0o755)
    if not precise:
        p = helpers/'python3'; p.write_text('#!/bin/sh\nexit 127\n'); p.chmod(0o755)
    path = str(helpers) + os.pathsep + os.environ['PATH']
    t = s.Terminal(d, Path('/usr/bin/env'), args=['TERM=xterm', 'COLORTERM=', 'PATH='+path, str(BINARY), str(book)])

    def copied(expected):
        for _ in range(150):
            if (d/'copied').exists() and (d/'copied').read_text() == expected: return
            time.sleep(.03)
        raise AssertionError((expected, (d/'copied').read_text() if (d/'copied').exists() else 'nothing copied'))

    def find(query):
        t.keys('Space','f'); t.see('Find in text layer'); t.keys('C-u'); t.type(query); t.keys('Enter'); t.see('Match 1 / 1')

    try:
        t.see('1 / 3')
        for _ in range(rotation//90): t.keys('Space','r','R')
        find('interoperability')
        t.keys('Space','c')
        t.wait('Text geometry ready', lambda f: 'NORMAL' in f and 'Loading text' not in f)
        t.keys('y','i','w'); copied('interoperability')
        if precise:
            (d/'copied').unlink()
            t.keys('v','4','l','y'); copied('inter')
            t.keys('g','v','Space','a','h'); t.see('Highlight'); t.keys('Enter'); t.see('Annotation saved')
            notes = json.loads(next((d/'state/botui/positions').glob('*.notes')).read_text())
            assert notes[0]['quote'] == 'inter' and len(notes[0]['pdf_regions']) == 1, notes
            assert notes[0]['end'][1] - notes[0]['byte'] == 5, notes
            t.keys('Space','c')
            find('Go to page three')
            t.keys('Space','c'); t.see('NORMAL')
            t.keys('Enter'); t.see('3 / 3')
            t.keys('Space','r','['); t.see('1 / 3')
            find('Open example.invalid')
            t.keys('Space','c'); t.see('NORMAL')
            t.keys('Enter'); t.see('External website'); t.keys('Up','Enter')
            for _ in range(100):
                if (d/'opened-url').exists(): break
                time.sleep(.03)
            assert (d/'opened-url').read_text() == 'https://example.invalid/botui-audit'
        print('PASS PDF interaction', 'characters and links' if precise else 'without optional GI', 'rotation', rotation, flush=True)
    finally:
        t.run('kill-server', required=False)


if __name__ == '__main__':
    gi = subprocess.run(['python3','-I','-c', "import gi; gi.require_version('Poppler','0.18'); from gi.repository import Poppler"], capture_output=True)
    with tempfile.TemporaryDirectory(prefix='botui-pdf-interaction-') as tmp:
        if gi.returncode == 0:
            for rotation in [0,90,180,270]: check(Path(tmp), True, rotation)
        else: print('SKIP exact PDF geometry: optional Poppler GI unavailable')
        check(Path(tmp), False)
