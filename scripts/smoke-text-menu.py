#!/usr/bin/env python3
"""All six right-click actions, with word targeting and selection preservation."""
import importlib.util, json, os, tempfile, time, zipfile
from pathlib import Path
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
binary=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()
with tempfile.TemporaryDirectory(prefix='botui-text-menu-') as tmp:
 root=Path(tmp);helpers=root/'bin';helpers.mkdir()
 for name in ['wl-copy','xclip','xsel','pbcopy']:
    p=helpers/name;p.write_text('#!/bin/sh\ncat > "$HOME/copied"\n');p.chmod(0o755)
 os.environ['PATH']=str(helpers)+os.pathsep+os.environ['PATH']
 for kind in ['fb2','epub']:
    d=root/kind;d.mkdir();config=d/'config/botui/config.toml';config.parent.mkdir(parents=True)
    config.write_text('''zen = true
[lookup]
dictionary = "Define fixture"
translation = "Translate fixture"
[[lookup.profiles]]
name = "Define fixture"
provider = "command"
command = ["printf", "Definition: %s", "{text}"]
[[lookup.profiles]]
name = "Translate fixture"
purpose = "translation"
provider = "command"
command = ["printf", "Translation: %s", "{text}"]
''')
    book=d/f'book.{kind}';prose='<p>Alpha bravo charlie delta.</p>'
    if kind=='fb2':book.write_text('<FictionBook><body><section>'+prose+'</section></body></FictionBook>')
    else:
      with zipfile.ZipFile(book,'w') as z:
        z.writestr('mimetype','application/epub+zip')
        z.writestr('META-INF/container.xml','<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
        z.writestr('book.opf','<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Menu</dc:title><dc:identifier id="id">menu</dc:identifier></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>')
        z.writestr('c.xhtml','<html xmlns="http://www.w3.org/1999/xhtml"><body>'+prose+'</body></html>')
    t=s.Terminal(d,binary,args=[str(book)])
    def xy(word):
        frame=t.see('Alpha bravo'); y,line=next((i,l) for i,l in enumerate(frame.splitlines(),1) if 'Alpha bravo' in l)
        return line.index(word)+1,y
    def mouse(pos,button=2,end='M'):
        x,y=pos;seq=f'\033[<{button};{x};{y}{end}'
        t.run('send-keys','-t','shelf','-H',*[f'{b:02x}' for b in seq.encode()])
    def menu(word='bravo'):
        pos=xy(word);mouse(pos);mouse(pos,end='m');f=t.see('Selected text')
        for label in ['Copy','Highlight','Search','Translate','Define','Add note']:assert label in f,f
    def choose(label):t.click(label,t.see('Selected text'))
    def copied(value):
        for _ in range(100):
            if (d/'copied').exists() and (d/'copied').read_text()==value:return
            time.sleep(.03)
        raise AssertionError((value,(d/'copied').read_text()))
    try:
        t.see('Alpha bravo');menu();choose('Copy');copied('bravo')
        menu();choose('Define');t.see('Definition: bravo');t.keys('Escape')
        menu();choose('Translate');t.see('Translation: bravo');t.keys('Escape')
        menu();choose('Search');t.see('Find');t.see('bravo');t.keys('Escape')
        menu();choose('Highlight');t.see('Highlight');t.keys('Enter');t.see('Annotation saved')
        menu('charlie');choose('Add note');t.see('Comment');t.type('Context note');t.keys('C-s');t.see('Annotation saved')
        data=json.loads(next((d/'state/botui/positions').glob('*.notes')).read_text())
        assert any(n['quote']=='bravo' and n['color'] for n in data),data
        assert any(n['quote']=='charlie' and n['body']=='Context note' for n in data),data
        a=xy('Alpha');b=xy('charlie');b=(b[0]+len('charlie')-1,b[1])
        mouse(a,0);mouse(b,32);mouse(b,0,'m')
        menu();choose('Copy');copied('Alpha bravo charlie')
        menu();t.keys('Escape');t.see('Alpha bravo');t.keys('q')
    finally:t.run('kill-server',required=False)
    print(f'PASS {kind}: all six context actions, word targeting, existing range and cancel')
