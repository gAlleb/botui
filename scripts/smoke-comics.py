#!/usr/bin/env python3
"""CBZ and real stored RAR4: natural page order, reader controls and persistence."""
import importlib.util, os, struct, subprocess, tempfile, zipfile, zlib, tomllib, json
from pathlib import Path
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
BINARY=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()

def png(rgb,level=6):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',120,160,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes(rgb)*120)*160,level))+chunk(b'IEND',b'')

def rar(items):
    # RAR4 stored files: header CRC16 is the low half of CRC32; method 0x30.
    def header(kind,flags,body):
        data=struct.pack('<BHH',kind,flags,len(body)+7)+body
        return struct.pack('<H',zlib.crc32(data)&0xffff)+data
    result=b'Rar!\x1a\x07\0'+header(0x73,0,b'\0'*6)
    for name,data in items:
        name=name.encode('ascii')
        body=struct.pack('<IIBIIBBHI',len(data),len(data),3,zlib.crc32(data),0,20,0x30,len(name),0o100644)+name
        result+=header(0x74,0x8000,body)+data
    return result+header(0x7b,0,b'')

def run(ext, fixture=None):
    with tempfile.TemporaryDirectory(prefix='botui-comic-') as tmp:
        root=Path(tmp);book=root/f'Panels.{ext}'
        items=[('pages/10.png',png((20,40,220))),('pages/2 [b].png',png((20,210,40))),('pages/1.png',png((220,30,40))),('__MACOSX/._1.png',b'junk'),('ComicInfo.xml',b'<ComicInfo/>')]
        if ext=='cbz':
            with zipfile.ZipFile(book,'w',compression=zipfile.ZIP_DEFLATED) as z:
                for name,data in items:z.writestr(name,data)
        else:
            book.write_bytes(fixture.read_bytes() if fixture else rar(items))
        config=root/'config/botui/config.toml';config.parent.mkdir(parents=True)
        config.write_text('zen = false\nreading_layout = "single"\nscan_theme = "original"\n')
        helpers=root/'bin';helpers.mkdir()
        for name in ['bsdtar','unrar','7z','7zz','unar']:
            helper=helpers/name;helper.write_text('#!/bin/sh\necho external-unpacker-called >> "$HOME/external-unpacker"\nexit 99\n');helper.chmod(0o755)
        os.environ['PATH']=str(helpers)+os.pathsep+os.environ['PATH']
        def start(): return s.Terminal(root,Path('/usr/bin/env'),args=['TERM=xterm','COLORTERM=',str(BINARY),str(book)])
        t=start()
        def ansi():return t.run('capture-pane','-p','-e','-t','shelf').stdout
        try:
            t.see(ext.upper());t.see('1 / 3')
            t.wait('First image',lambda _: '220;30;40' in ansi())
            t.keys('Right');t.see('2 / 3');t.wait('Natural second image',lambda _: '20;210;40' in ansi())
            t.keys('Right');t.see('3 / 3');t.wait('Natural tenth image',lambda _: '20;40;220' in ansi())
            t.keys('Space');frame=t.see('Reading tools')
            assert 'Dictionary' not in frame and 'Text cursor' not in frame and 'Find in book' not in frame,frame
            t.keys('a');frame=t.see('Page note');assert 'Highlight]' not in frame
            t.keys('p');t.see('Note');t.type('Comic page note');t.keys('Enter')
            t.keys('Space','b','a');t.see('Bookmark')
            t.keys('Space','t');t.see('Page theme');t.keys('Home','Down','Enter');t.see('Inverted colors')
            assert tomllib.loads(config.read_text())['scan_theme']=='inverted'
            t.keys('Space','t','Home','Enter');t.see('Original colors')
            t.keys('q')
        finally:t.run('kill-server',required=False)
        t=start()
        try:
            t.see('3 / 3');t.keys('Space','a','a');t.see('Comic page note');t.keys('Escape')
            t.keys('Space','o');frame=t.see('Contents')
            assert frame.index('1.png') < frame.index('2 [b].png') < frame.index('10.png'),frame
            t.keys('q')
        finally:t.run('kill-server',required=False)
        # The local shelf discovers the extension and uses the first page as cover.
        t=s.Terminal(root,BINARY,args=[str(root)])
        try:t.see('Panels');t.see(ext.upper())
        finally:t.run('kill-server',required=False)
        assert not (root/'external-unpacker').exists(), 'CBR invoked an external unpacker'
        print(f'PASS {fixture.name if fixture else ext}: natural order, image rendering, menus, notes, bookmark, theme, reopen and local shelf')

if __name__=='__main__':
    run('cbz');run('cbr')
    for fixture in sorted((Path(__file__).parent/'fixtures/comics').glob('*.cbr')):run('cbr',fixture)
