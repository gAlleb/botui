#!/usr/bin/env python3
"""Rendered Markdown, syntax colors, local image, source toggle and library discovery."""
import importlib.util, os, struct, tempfile, zlib
from pathlib import Path
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
binary=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()
def png():
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',120,40,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes((20,210,40))*120)*40))+chunk(b'IEND',b'')
with tempfile.TemporaryDirectory(prefix='botui-markdown-') as tmp:
    root=Path(tmp); config=root/'config/botui/config.toml';config.parent.mkdir(parents=True)
    config.write_text('zen = false\nreading_layout = "single"\ntheme = "slate-dark"\n')
    (root/'diagram.png').write_bytes(png())
    book=root/'Sample.MD';book.write_text('''# Markdown demo

Alpha **bravo** charlie. Привет мир.

> Quoted wisdom

- [x] Finished task

| Name | Value |
| --- | --- |
| Answer | 42 |

```rust
let answer = 42; // comment
```

![Local diagram](diagram.png)
''')
    t=s.Terminal(root,Path('/usr/bin/env'),args=['TERM=xterm','COLORTERM=',str(binary),str(book)])
    def ansi(): return t.run('capture-pane','-p','-e','-t','shelf').stdout
    try:
        t.run('resize-window','-t','shelf','-x','115','-y','60')
        frame=t.see('Alpha bravo charlie')
        for word in ['Quoted wisdom','Finished task','Answer','let answer','Local diagram']: t.see(word)
        t.wait('Syntax colors',lambda _: '140;185;255' in ansi() and '235;180;120' in ansi())
        t.wait('Local illustration',lambda _: '20;210;40' in ansi())
        t.click('F9 Source',t.see('F9 Source')); t.see('**bravo**'); t.see('```rust');t.see('| --- | --- |')
        t.click('F9 Formatted',t.see('F9 Formatted')); t.see('Alpha bravo charlie')
        t.keys('Space','r');t.see('Formatted / source'); t.keys('v');t.see('**bravo**')
        t.keys('F9');t.see('Alpha bravo charlie')
        t.keys('Space','f');t.type('Alpha bravo charlie');t.keys('Enter');t.see('Alpha bravo charlie')
        t.keys('Escape','Space','q'); t.run('kill-server',required=False)
        t=s.Terminal(root,binary,args=[str(root)]); t.see('Sample');t.see('Markdown')
        print('PASS Markdown: formatting, table, syntax colors, local image, mouse/Space/F9 source toggle, search and local shelf')
    finally: t.run('kill-server',required=False)
