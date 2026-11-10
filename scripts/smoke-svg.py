#!/usr/bin/env python3
"""Inline/linked EPUB SVGs use cached Kitty images in the ordinary reader.
No desktop windows, server accounts or personal reading state are touched.
"""
import base64, fcntl, json, os, pty, re, select, signal, struct, sys, tempfile, termios, time, zipfile, zlib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def book(path, album=False):
    svg='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 180"><rect width="360" height="180" fill="white"/><path d="M20 160L180 20L340 160Z" fill="#ff0000"/><text x="130" y="170" font-size="20" fill="blue">SVG label</text></svg>'
    prose='' if album else '<p>SVGSTART Before the diagram.</p>'
    tail='' if album else '<p>SVGEND After the diagram.</p>'
    html=f'<html xmlns="http://www.w3.org/1999/xhtml"><body>{prose}{svg}{tail}<img src="diagram.svg"/>{tail}</body></html>'
    with zipfile.ZipFile(path,'w') as z:
        z.writestr('META-INF/container.xml','<container><rootfile full-path="book.opf"/></container>')
        z.writestr('book.opf','<package><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/><item id="s" href="diagram.svg" media-type="image/svg+xml"/></manifest><spine><itemref idref="c"/></spine></package>')
        z.writestr('c.xhtml',html);z.writestr('diagram.svg',svg)

def check(binary, folder, album=False):
    (folder/'botui').mkdir()
    (folder/'botui/config.toml').write_text('reading_layout="scroll"\nzen=true\nzen_progress=true\n')
    source=folder/'svg.epub';book(source,album)
    env=dict(os.environ,XDG_CONFIG_HOME=str(folder),XDG_STATE_HOME=str(folder),XDG_CACHE_HOME=str(folder),TERM='xterm-kitty')
    for k in ('TMUX','STY','NO_COLOR','TERM_PROGRAM','KITTY_WINDOW_ID'):env.pop(k,None)
    pid,fd=pty.fork()
    if pid==0:
        fcntl.ioctl(0,termios.TIOCSWINSZ,struct.pack('HHHH',45,120,1200,900))
        os.execve(str(binary),[str(binary),str(source)],env)
    images=[]; pending=bytearray(); active=None; stream=bytearray()
    def collect(seconds=1):
        nonlocal active
        start=time.monotonic()
        while time.monotonic()-start<seconds:
            if not select.select([fd],[],[],.01)[0]:continue
            data=os.read(fd,262144);stream.extend(data);pending.extend(data)
            if b'\x1b]11;?' in data:os.write(fd,b'\x1b]11;rgb:2020/2020/2020\x07\x1b[?1;2c')
            if b'\x1b[5n' in data:os.write(fd,b'\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b[0n')
            consumed=0
            for match in re.finditer(rb'\x1b_G([^;]*);(.*?)\x1b\\',pending,re.S):
                consumed=match.end();params=dict(p.split(b'=',1) for p in match[1].split(b',') if b'=' in p)
                if params.get(b'a') in (b'T',b't') and b's' in params:active=[params,bytearray()]
                if active:
                    active[1].extend(match[2])
                    if params.get(b'm',b'0')==b'0':
                        p,payload=active;active=None
                        pixels=base64.b64decode(payload)
                        if p.get(b'o')==b'z':pixels=zlib.decompress(pixels)
                        images.append((p,pixels))
            del pending[:consumed]
    try:
        collect(3)
        assert images,'SVG did not reach the terminal'
        p,pixels=images[0];assert p.get(b'f')==b'32',p
        red=sum(1 for r,g,b,a in struct.iter_unpack('BBBB',pixels) if r>200 and g<50 and b<50 and a>128)
        assert red>500, 'SVG path is missing or a raster placeholder was shown'
        blue=sum(1 for r,g,b,a in struct.iter_unpack('BBBB',pixels) if b>150 and r<100 and g<100 and a>128)
        assert blue>10, 'SVG text labels are missing'
        if not album:
            assert b'SVGSTART' in stream and b'SVGEND' in stream, 'Diagram displaced surrounding prose'
            # Keep the same diagrams visible; no new image uploads on every scroll step.
            before=len(images)
            for _ in range(3):os.write(fd,b'\x1b[B');collect(.12)
            for _ in range(3):os.write(fd,b'\x1b[A');collect(.12)
            assert len(images)==before,(before,len(images))
        else:
            os.write(fd,b'\x1b[C');collect(1)
            assert b'unavailable' not in stream.lower(),'SVG album page failed to decode'
        print(json.dumps({'SVG album' if album else 'SVG in prose':'PASS','uploads':len(images),'red_pixels':red}),flush=True)
    finally:
        os.kill(pid,signal.SIGTERM);os.waitpid(pid,0);os.close(fd)

if __name__=='__main__':
    for binary in sys.argv[1:] or [str(ROOT/'target/debug/botui')]:
        for album in [False,True]:
            with tempfile.TemporaryDirectory(prefix='botui-svg-') as temp:check(Path(binary).resolve(),Path(temp),album)
