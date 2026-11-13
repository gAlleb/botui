#!/usr/bin/env python3
"""Search every occurrence, not just pages, and keep Vim on the found word."""
import importlib.util
import os
from pathlib import Path
import re
import tempfile
import zipfile

spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
BINARY = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui')).resolve()


def cells(ansi):
    reverse = False
    out = []
    for token in re.split(r'(\x1b\[[0-9;]*m)', ansi):
        if token.startswith('\x1b['):
            codes = [int(c or 0) for c in token[2:-1].split(';')]
            i = 0
            while i < len(codes):
                c = codes[i]
                if c == 0 or c == 27:
                    reverse = False
                elif c == 7:
                    reverse = True
                elif c in (38, 48, 58) and i + 1 < len(codes):
                    i += 4 if codes[i + 1] == 2 else 2
                i += 1
        else:
            out.extend((c, reverse) for c in token)
    return out


def selected(t, suffix):
    def check(_):
        data = cells(t.run('capture-pane', '-p', '-e', '-t', 'shelf').stdout)
        plain = ''.join(c for c, _ in data)
        for m in re.finditer('lighthouse ' + suffix, plain):
            if all(r for _, r in data[m.start():m.start() + len('lighthouse')]):
                return True
        return False
    t.wait('Highlighted occurrence ' + suffix, check)


def text_search(root, extension, mode):
    d = root / (extension + mode)
    (d / 'config/botui').mkdir(parents=True)
    (d / 'config/botui/config.toml').write_text(f'reading_layout = "{mode}"\n')
    paragraphs = ['lighthouse alpha lighthouse bravo', 'lighthouse charlie']
    paragraphs += [f'Filler paragraph {i:03}.' for i in range(60)]
    paragraphs += ['Last lighthouse delta.']
    book = d / ('book.' + extension)
    if extension == 'md':
        book.write_text('\n\n'.join(paragraphs))
    elif extension == 'fb2':
        book.write_text('<FictionBook><body><section>' + ''.join('<p>' + p + '</p>' for p in paragraphs) + '</section></body></FictionBook>')
    else:
        with zipfile.ZipFile(book, 'w') as z:
            z.writestr('mimetype', 'application/epub+zip')
            z.writestr('META-INF/container.xml', '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
            z.writestr('book.opf', '<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Search</dc:title><dc:language>en</dc:language></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>')
            z.writestr('c.xhtml', '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Search</title></head><body>' + ''.join('<p>' + p + '</p>' for p in paragraphs) + '</body></html>')
    t = s.Terminal(d, BINARY, args=[str(book)])
    try:
        t.see('lighthouse alpha')
        t.keys('Space', 'f'); t.see('Find:'); t.type('lighthouse'); t.see('4 matches'); t.keys('Enter')
        selected(t, 'alpha')
        for key, word in [('F3', 'bravo'), ('F3', 'charlie'), ('F3', 'delta'), ('F3', 'alpha'), ('F4', 'delta')]:
            t.keys(key); selected(t, word)
        t.keys('F3'); selected(t, 'alpha')
        t.keys('Space', 'c'); t.see('NORMAL')
        t.keys('n'); selected(t, 'bravo')
        t.keys('N'); selected(t, 'alpha')
        t.keys('/', 'C-u'); t.type('lighthouse'); t.keys('Enter'); selected(t, 'alpha')
        # Moving the Vim cursor past a hit must change the origin for n/N.
        t.keys('w', 'w', 'w', 'n'); selected(t, 'charlie')
        t.keys('N'); selected(t, 'bravo')
        t.keys('F5'); t.see('4 matches'); t.keys('End', 'Enter'); selected(t, 'delta')
        print('PASS', extension, mode, flush=True)
    finally:
        t.run('kill-server', required=False)


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='botui-search-occurrences-') as tmp:
        for ext in ['fb2', 'epub', 'md']:
            for mode in ['single', 'spread', 'scroll']:
                text_search(Path(tmp), ext, mode)
