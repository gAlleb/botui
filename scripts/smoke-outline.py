#!/usr/bin/env python3
"""Read a real nested PDF outline, fold it, navigate, and reopen at its child page."""
import importlib.util
from pathlib import Path
import tempfile
import re
import time
import subprocess

repo = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('shelf', repo/'scripts/smoke-shelf.py')
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

# Rebuild the tiny PDF's object table with a native three-level outline.
objects = re.findall(rb'\d+ 0 obj\n(.*?)\nendobj', shelf.pdf(True), re.S)
objects[0] = b'<< /Type /Catalog /Pages 2 0 R /Outlines 9 0 R >>'
objects += [b'<< /Type /Outlines /First 10 0 R /Last 10 0 R /Count 4 >>',
 b'<< /Title (RootChapter) /Parent 9 0 R /First 11 0 R /Last 13 0 R /Count 3 /Dest [3 0 R /Fit] >>',
 b'<< /Title (ChildSection) /Parent 10 0 R /Next 13 0 R /First 12 0 R /Last 12 0 R /Count 1 /Dest [3 0 R /Fit] >>',
 b'<< /Title (LeafSection) /Parent 11 0 R /Dest [5 0 R /Fit] >>',
 b'<< /Title (FinalChapter) /Parent 10 0 R /Prev 11 0 R /Dest [7 0 R /Fit] >>']
data = bytearray(b'%PDF-1.4\n'); offsets = [0]
for i, obj in enumerate(objects, 1):
    offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
offset = len(data); data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
for pos in offsets[1:]: data.extend(f'{pos:010} 00000 n \n'.encode())
data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode())
with tempfile.TemporaryDirectory(prefix='botui-outline-') as d:
    root = Path(d); book = root/'nested.pdf'; book.write_bytes(data)
    # A real DjVu bookmark stream exercises hierarchy and quoted title parsing too.
    ppm = root/'one.ppm'; ppm.write_bytes(b'P6\n20 20\n255\n'+bytes([240,240,240])*400)
    one = root/'one.djvu'; subprocess.run(['c44',str(ppm),str(one)],check=True)
    djvu = root/'nested.djvu'; subprocess.run(['djvm','-c',str(djvu),str(one),str(one),str(one)],check=True)
    outlines = root/'outline.txt'
    outlines.write_text('(bookmarks ("RootChapter" "#1" ("ChildSection" "#1" ("LeafSection" "#2")) ("FinalChapter" "#3")))')
    subprocess.run(['djvused',str(djvu),'-s','-e',f'set-outline {outlines}'],check=True)
    for book, reopen in [(book,False),(book,True),(djvu,False),(djvu,True)]:
        term = shelf.Terminal(root, repo/'target/debug/botui', args=[str(book)])
        try:
            frame = term.see('LeafSection')
            positions = [next(line.index(word) for line in frame.splitlines() if word in line) for word in ('RootChapter','ChildSection','LeafSection')]
            assert positions[0] < positions[1] < positions[2], frame
            if reopen:
                term.see('2 / 3')
                # The whole outline remains visible, with current section selected.
                ansi = term.run('capture-pane','-p','-e','-t','shelf').stdout
                leaf = next(line for line in ansi.splitlines() if 'LeafSection' in line)
                assert '\x1b[7m' in leaf or re.search(r'\x1b\[[0-9;]*7(?:;[0-9]+)*m',leaf), repr(leaf)
                continue
            term.keys('Tab','Left','Left','Left')
            term.wait('Fold root',lambda s:'▸ 1  RootChapter' in s and 'LeafSection' not in s)
            term.keys('Right','Down','Right')
            term.see('LeafSection')
            term.click('LeafSection',term.see('LeafSection'))
            term.see('2 / 3')
            term.keys('q')
            for _ in range(100):
                if term.run('has-session','-t','shelf',required=False).returncode: break
                time.sleep(.03)
            else: raise AssertionError('Reader did not close')
        finally: term.run('kill-server',required=False)
print('PDF/DjVu outline: three levels, folding, page navigation and resume PASS')
