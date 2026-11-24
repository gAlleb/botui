#!/usr/bin/env python3
"""Inline/linked EPUB SVGs use cached Kitty images in the ordinary reader.
No desktop windows, server accounts or personal reading state are touched.
"""
import base64, fcntl, json, os, pty, re, select, signal, struct, sys, tempfile, termios, time, zipfile, zlib
from pathlib import Path
from unicodedata import combining
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

def illustrated_page_turn(path):
    # A page and a half of short paragraphs put the wide image on page two.
    with zipfile.ZipFile(path, 'w') as archive:
        archive.writestr('META-INF/container.xml', '<container><rootfile full-path="book.opf"/></container>')
        archive.writestr('book.opf', '<package><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>'
                         '<item id="n" href="notes.xhtml" media-type="application/xhtml+xml"/>'
                         '<item id="fig" href="banner.svg" media-type="image/svg+xml"/></manifest>'
                         '<spine><itemref idref="c"/></spine></package>')
        archive.writestr('banner.svg', '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 120">'
                         '<rect width="960" height="120" fill="red"/></svg>')
        archive.writestr('c.xhtml', '<html><body>' + '<p>Approaching the illustration.</p>' * 27
                         + '<p>Passage <a href="notes.xhtml#n">[1]</a></p>'
                         '<img src="banner.svg" style="width:100%"/><p>After the figure.</p></body></html>')
        archive.writestr('notes.xhtml', '<html><body><aside id="n"><p>Explanation of the passage.</p></aside></body></html>')


def check(binary, folder, album=False, page_turn=False):
    (folder/'botui').mkdir()
    (folder/'botui/config.toml').write_text('reading_layout="scroll"\nzen=true\nzen_progress=true\n')
    source=folder/'svg.epub'
    if page_turn:
        (folder/'botui/config.toml').write_text('reading_layout="single"\nzen=true\npage_width=112\nparagraph_spacing=1\n')
        illustrated_page_turn(source)
    else:
        book(source,album)
    env=dict(os.environ,XDG_CONFIG_HOME=str(folder),XDG_STATE_HOME=str(folder),XDG_CACHE_HOME=str(folder),TERM='xterm-kitty')
    for k in ('TMUX','STY','NO_COLOR','TERM_PROGRAM','KITTY_WINDOW_ID'):env.pop(k,None)
    pid,fd=pty.fork()
    if pid==0:
        fcntl.ioctl(0,termios.TIOCSWINSZ,struct.pack('HHHH',45,120,1200,900))
        os.execve(str(binary),[str(binary),str(source)],env)
    images=[]; pending=bytearray(); active=None; stream=bytearray()
    resize_on_query = None
    def collect(seconds=1):
        nonlocal active, resize_on_query
        start=time.monotonic()
        while time.monotonic()-start<seconds:
            if not select.select([fd],[],[],.01)[0]:continue
            data=os.read(fd,262144);stream.extend(data);pending.extend(data)
            if b'\x1b]11;?' in data:os.write(fd,b'\x1b]11;rgb:2020/2020/2020\x07\x1b[?1;2c')
            if b'\x1b[5n' in data:
                if resize_on_query:
                    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', *resize_on_query))
                    resize_on_query = None
                os.write(fd,b'\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b[0n')
            consumed=0
            for match in re.finditer(rb'\x1b_G([^;\x1b]*)(?:;([^\x1b]*))?\x1b\\',pending):
                consumed=match.end();params=dict(p.split(b'=',1) for p in match[1].split(b',') if b'=' in p)
                if params.get(b'a') in (b'T',b't') and b's' in params:active=[params,bytearray()]
                if active:
                    active[1].extend(match[2] or b'')
                    if params.get(b'm',b'0')==b'0':
                        p,payload=active;active=None
                        pixels=base64.b64decode(payload)
                        if p.get(b'o')==b'z':pixels=zlib.decompress(pixels)
                        images.append((p,pixels))
            del pending[:consumed]
    try:
        collect(3)
        if page_turn:
            assert not images and b'Approaching' in stream
            stream.clear()
            os.write(fd, b'\x1b[C')
            collect(1)
            first_frame = stream.split(b'\x1b[?2026l', 1)[0]
            pixel = '\U0010eeee'
            assert b'Passage' in first_frame and pixel.encode() in first_frame, 'First frame lacks the prefetched illustration'
            os.write(fd, b' rf')
            collect(.4)
            stream.clear()
            os.write(fd, b'\r')
            collect(.5)
            assert b'Explanation' in stream and b'passage.' in stream, 'Footnote did not open'
            # A Kitty row normally starts at column zero (U+0305). Its first
            # uncovered cell after the note must explicitly resume farther right.
            painted = stream.decode('utf-8', errors='replace')
            cells = painted.split(pixel)[1:]
            assert any(len(cell) >= 3 and all(combining(c) for c in cell[:3])
                       and cell[1] != '\u0305' for cell in cells), 'Missing image coordinates after the note'
            print('PASS illustration on first page-turn frame and visible beyond the footnote', flush=True)
            return
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
            # Changing the font changes the terminal grid. A clear published
            # outside the redraw exposes a blank screen while prose reflows.
            for columns in (100, 120):
                stream.clear()
                fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 45, columns, 1200, 900))
                collect(.8)
                assert b'\x1b[5n' not in stream, 'Repeated graphics probe for an exact font size'
                frames = re.findall(rb'\x1b\[\?2026h(.*?)\x1b\[\?2026l', stream, re.S)
                clear = b'\x1b[2J'
                assert any(clear in frame and b'SVGSTART' in frame for frame in frames), 'Incomplete resize frame'
                assert stream.count(clear) == sum(frame.count(clear) for frame in frames), 'Unprotected resize clear'
                assert len(images) == before, 'Font zoom uploaded the same illustration again'
                placements = re.findall(rb'\x1b_Ga=p,U=1,i=(\d+),p=\d+,c=(\d+),r=(\d+)', stream)
                assert placements and all(any(p[b'i'] == i for p, _ in images) for i, _, _ in placements), 'Missing resized image placement'
            # A 900px window with 40 rows needs an explicit cell-size reply.
            # Resize again during that probe; only the final grid should be painted.
            stream.clear()
            resize_on_query = (31, 90, 1200, 900)
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 1200, 900))
            collect(1)
            assert resize_on_query is None and b'SVGSTART' in stream
            assert stream.count(b'\x1b[5n') == 2, 'Reused cell metrics from a superseded grid'
            painted_rows = re.findall(rb'\x1b\[(\d+);\d+H', stream)
            assert max(map(int, painted_rows)) == 31, 'Published a page for the previous font size'
            # Some PTYs report only character dimensions, even for graphics terminals.
            for columns in (100, 105):
                stream.clear()
                fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 35, columns, 0, 0))
                collect(.6)
                assert b'\x1b[5n' in stream and b'SVGSTART' in stream, 'Stale unknown cell size'
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
        with tempfile.TemporaryDirectory(prefix='botui-svg-turn-') as temp:
            check(Path(binary).resolve(), Path(temp), page_turn=True)
