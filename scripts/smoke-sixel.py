#!/usr/bin/env python3
"""Exercise the Foot/Sixel path in an isolated PTY: popup repaint and cached artwork."""
import fcntl, importlib.util, os, pty, re, select, signal, struct, sys, tempfile, termios, time, zipfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('shelf', ROOT / 'scripts/smoke-shelf.py')
shelf = importlib.util.module_from_spec(spec); spec.loader.exec_module(shelf)
SIXEL = re.compile(rb'\x1bP[^q\x1b]*q')


def check(binary, comic):
    with tempfile.TemporaryDirectory(prefix='botui-sixel-') as temp:
        root = Path(temp); (root/'botui').mkdir()
        (root/'botui/config.toml').write_text('zen=true\nzen_progress=true\nopaque_background=true\nscan_theme="original"\n')
        book = root / ('pages.cbz' if comic else 'pages.pdf')
        if comic:
            with zipfile.ZipFile(book, 'w') as archive:
                for i in range(3): archive.writestr(f'{i}.png', shelf.ORIGINAL_COVER)
        else: book.write_bytes(shelf.pdf(True))
        env = dict(os.environ, XDG_CONFIG_HOME=temp, XDG_STATE_HOME=temp, XDG_CACHE_HOME=temp, TERM='foot')
        for key in ('TMUX', 'STY', 'NO_COLOR', 'TERM_PROGRAM'): env.pop(key, None)
        child, terminal = pty.fork()
        if child == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 1200, 800))
            os.execve(str(binary), [str(binary), str(book)], env)
        def collect(timeout=12):
            data=bytearray(); start=last=time.monotonic(); tail=b''
            while time.monotonic()-start < timeout:
                if select.select([terminal],[],[],.01)[0]:
                    chunk=os.read(terminal,262144);data.extend(chunk);last=time.monotonic()
                    query=tail+chunk
                    if b'\x1b]11;?' in query:
                        os.write(terminal,b'\x1b]10;rgb:dddd/dddd/dddd\x07\x1b]11;rgb:2222/2222/2222\x07')
                    if b'\x1b[c' in query: os.write(terminal,b'\x1b[?62;4c')
                    if b'\x1b[5n' in query: os.write(terminal,b'\x1b[6;20;10t\x1b[0n')
                    tail=query[-5:]
                elif data.endswith(b'\x1b[?2026l') and time.monotonic()-last > .25:
                    return bytes(data)
            raise AssertionError('Sixel frame did not finish')
        def send(keys): os.write(terminal,keys);return collect()
        try:
            assert SIXEL.search(collect()), 'Sixel was not selected'
            title=b'Comic background' if comic else b'Page theme'
            assert title in send(b' t')
            for keys in (b'\x1b[B',b'\x1b[B'):
                preview=send(keys)
                assert title in preview and b'Cancel' in preview, 'Incomplete popup repaint'
                if comic: assert not SIXEL.search(preview), 'Background resent unchanged comic pixels'
                else:
                    assert SIXEL.search(preview), 'PDF preview did not change pixels'
                    for frame in preview.split(b'\x1b[?2026h')[1:]:
                        if SIXEL.search(frame): assert title in frame and b'Cancel' in frame, 'Image erased the dialog'
            assert SIXEL.search(send(b'\x1b')), 'Closing the popup did not restore its covered pixels'
            print('PASS Sixel', 'comic background' if comic else 'PDF preview', 'and complete popup restoration', flush=True)
        finally:
            os.kill(child,signal.SIGTERM);os.waitpid(child,0);os.close(terminal)


if __name__ == '__main__':
    binary=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else ROOT/'target/debug/botui'
    for comic in (False, True): check(binary,comic)
