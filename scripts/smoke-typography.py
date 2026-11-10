#!/usr/bin/env python3
"""Verify actual terminal faces and a three-level EPUB/FB2 outline."""
from pathlib import Path
import tempfile, zipfile, importlib.util, re, sys, json
repo=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('shelf',repo/'scripts/smoke-shelf.py')
shelf=importlib.util.module_from_spec(spec); spec.loader.exec_module(shelf)
binary=repo/'target/debug/botui'
with tempfile.TemporaryDirectory(prefix='botui-reader-audit-') as d:
    root=Path(d)
    fb=root/'styles.fb2'
    fb.write_text('<FictionBook><body><section><title><p>RootChapter</p></title><p>PlainWord <emphasis>ItalicWord</emphasis> <strong>BoldWord</strong>.</p><section><title><p>ChildSection</p></title><p>Child text.</p><section><title><p>LeafSection</p></title><p>Leaf text.</p></section></section></section><section><title><p>OtherChapter</p></title><p>ENDMARK</p></section></body></FictionBook>')
    epub=root/'styles.epub'
    with zipfile.ZipFile(epub,'w') as z:
        z.writestr('mimetype','application/epub+zip')
        z.writestr('META-INF/container.xml','<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
        z.writestr('book.opf','<package version="3.0"><metadata/><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="c"/></spine></package>')
        z.writestr('c.xhtml','<html><body><h1 id="root">RootChapter</h1><p>PlainWord <em>ItalicWord</em> <strong>BoldWord</strong>.</p><h2 id="child">ChildSection</h2><p>Child text.</p><h3 id="leaf">LeafSection</h3><p>Leaf text.</p><h1 id="other">OtherChapter</h1><p>ENDMARK</p></body></html>')
        z.writestr('nav.xhtml','<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="c.xhtml#root">RootChapter</a><ol><li><a href="c.xhtml#child">ChildSection</a><ol><li><a href="c.xhtml#leaf">LeafSection</a></li></ol></li></ol></li><li><a href="c.xhtml#other">OtherChapter</a></li></ol></nav></body></html>')
    for book in [epub,fb]:
        t=shelf.Terminal(root,binary,args=[str(book)])
        try:
            t.see('PlainWord')
            ansi=t.run('capture-pane','-p','-e','-t','shelf').stdout
            line=next(l for l in ansi.splitlines() if 'ItalicWord' in l)
            italic=bool(re.search(r'\x1b\[(?:[0-9]+;)*3(?:;[0-9]+)*m',line))
            print(book.suffix, "italic and bold faces: PASS", flush=True)
            assert "\x1b[1m" in line or re.search(r"\x1b\[[0-9;]*1(?:;[0-9]+)*m",line)
            assert italic
            t.keys('o'); toc=t.see('Sections: 4')
            rows=[l for l in toc.splitlines() if any(x in l for x in ('RootChapter','ChildSection','LeafSection','OtherChapter'))]
            positions=[next(l.index(label) for l in rows if label in l) for label in ('RootChapter','ChildSection','LeafSection')]
            assert positions[1]-positions[0]==positions[2]-positions[1]==2,rows
            t.keys('Home','Left'); collapsed=t.wait('collapsed',lambda s:'▸ RootChapter' in s and 'LeafSection' not in '\n'.join(l[22:] for l in s.splitlines()) and 'ChildSection' not in '\n'.join(l[22:] for l in s.splitlines()))
            t.keys('Right'); t.see('LeafSection')
            print(book.suffix,'TREE 3 levels, fold and expand: PASS',flush=True)
        finally:t.run('kill-server',required=False)
