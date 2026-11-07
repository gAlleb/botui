#!/usr/bin/env python3
"""Exercise independent OPDS navigation and actual PDF/DjVu rendering in a private terminal."""
import base64
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('shelf', ROOT/'scripts/smoke-shelf.py')
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = ROOT/'target/debug/botui'

def pdf():
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Count 3 /Kids [3 0 R 5 0 R 7 0 R] >>']
    for i in range(3):
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 440] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {4+2*i} 0 R >>'.encode())
        content = f'0.{i+2} 0.4 0.6 rg {28*i} 30 {220-i*28} {350-i*40} re f'.encode()
        if i==1:content+=b" BT /F1 16 Tf 10 32 Td (Lighthouse atlas) Tj ET"
        objects.append(f'<< /Length {len(content)} >>\nstream\n'.encode()+content+b'\nendstream')
    data=bytearray(b'%PDF-1.4\n'); offsets=[0]
    for i,obj in enumerate(objects,1):
        offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
    offset=len(data);data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for pos in offsets[1:]:data.extend(f'{pos:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode());return data

class Term(shelf.Terminal):
    def __init__(self,tmp,*args):
        self.base=['tmux','-f','/dev/null','-S',str(tmp/'term.sock')]
        self.env=dict(HOME=str(tmp),PATH=os.environ['PATH'],LC_ALL='C.UTF-8',XDG_STATE_HOME=str(tmp/'state'))
        self.run('new-session','-d','-s','shelf','-x','100','-y','25',shlex.join([str(BINARY),*map(str,args)]))

class Feed(BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_GET(self):
        if self.headers.get('Authorization')!='Basic '+base64.b64encode(b'user:testpass').decode():
            self.send_error(401);return
        atom='<feed xmlns="http://www.w3.org/2005/Atom">'
        pages={'/start':(atom+'<title>Каталог OPDS</title><link rel="search" type="application/opensearchdescription+xml" href="search.xml"/><entry><title>Книги маяка</title><link rel="subsection" type="application/atom+xml" href="shelf"/></entry></feed>').encode(),
               '/shelf':(atom+'<title>Моя полка OPDS</title><entry xml:base="/pub/"><title>Атлас пролива</title><author><name>Иванов</name></author><link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="first.epub"/><link rel="http://opds-spec.org/image/thumbnail" href="cover"/></entry><link rel="next" href="next"/></feed>').encode(),
               '/next':(atom+'<title>Следующий выпуск</title><entry><title>Continuation</title><link rel="subsection" type="application/atom+xml" href="shelf"/></entry><link rel="previous" href="shelf"/></feed>').encode(),
               '/search.xml':b'<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/"><Url type="application/atom+xml" template="/find?q={searchTerms}"/></OpenSearchDescription>',
               '/find?q=Amber%20%26%20fire':(atom+'<title>Results</title><entry><title>Verified match</title><link rel="subsection" type="application/atom+xml" href="shelf"/></entry></feed>').encode(),
               '/pub/first.epub':shelf.epub(),'/pub/cover':(ROOT/'scripts/fixtures/shelf-cover.jpg').read_bytes()}
        data=pages.get(self.path)
        if not data:self.send_error(404);return
        self.server.seen.add(self.path);self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

def opds(tmp):
    with ThreadingHTTPServer(('127.0.0.1',0),Feed) as server:
        server.seen=set(); threading.Thread(target=server.serve_forever,daemon=True).start()
        t=Term(tmp,'--opds',f'http://127.0.0.1:{server.server_port}/start')
        try:
            t.see('OPDS catalog');t.keys('Tab');t.type('user');t.keys('Tab');t.type('testpass');t.keys('Enter')
            t.click('Книги маяка',t.see('Книги маяка'));t.see('Атлас пролива');t.see('▀')
            t.see('Continuation');t.keys('PageDown','PageUp');t.see('Атлас пролива')
            t.click('Атлас пролива',t.see('Атлас пролива'));t.click('EPUB',t.wait('Форматы книги OPDS',lambda s:'EPUB' in s and '[V ' not in s))
            t.see(shelf.EPUB_MARK);t.keys('q');t.see('EPUB');t.keys('Escape');t.see('Моя полка OPDS')
            t.keys('/');t.see('Search OPDS catalog');t.type('Amber & fire');t.keys('Enter');t.see('Verified match')
            assert server.seen=={'/search.xml','/find?q=Amber%20%26%20fire','/start','/shelf','/next','/pub/cover','/pub/first.epub'},server.seen
        finally:t.run('kill-server',required=False);server.shutdown()

def scan(tmp,path):
    t=Term(tmp,path)
    try:
        t.see('P Read');t.keys('p')
        first=t.wait('Первая страница скана',lambda s:'▀' in s or '▄' in s)
        t.keys('Right');t.wait('Другое изображение на второй странице',lambda s:s!=first and ('▄' in s or '▀' in s))
        t.keys('+');t.wait('Увеличенная страница',lambda s:'▄' in s or '▀' in s);t.keys('Down','-');t.keys('m');t.see('2 / 3');t.click('Two-page spread',t.see('Two-page spread'))
        t.wait('Возврат из меню в разворот',lambda s:'Two-page spread' not in s and ('▀' in s or '▄' in s))
        t.keys('Home','/');t.see('Find in text layer');t.type('lighthouse');t.keys('Enter');t.see('1 matching page')
        t.keys('b');t.see('Page bookmarked');t.keys('i');t.see('Page note');t.type('Watch the lighthouse');t.keys('Enter')
        t.keys('Home','g');t.see('Page bookmarks');t.see('Page 2');t.keys('Enter')
        t.keys('a');t.see('Watch the lighthouse');t.keys('Enter','e');t.see('Saved ')
        exports=tmp/'.local/share/botui/exports';assert any('## Page 2' in p.read_text() and 'Watch the lighthouse' in p.read_text() for p in exports.glob('*.md'))
        t.keys('s');t.see('Total time book open');t.keys('Escape');t.wait('Statistics closed',lambda s:'Total time book open' not in s)
        t.keys('m');t.see('2 / 3');t.keys('q')
        import time
        for _ in range(100):
            if t.run('has-session',required=False).returncode:break
            time.sleep(.025)
    finally:t.run('kill-server',required=False)
    t=Term(tmp,path)
    try:t.see('P Read');t.keys('p');t.wait('Восстановление скана',lambda s:'▀' in s or '▄' in s);t.keys('g');t.see('Page bookmarks');t.see('Page 2');t.keys('Enter');t.keys('s');frame=t.see('Sessions on this device');assert 'Sessions on this device  2' in frame,frame
    finally:t.run('kill-server',required=False)

if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='botui-sources-') as name:
        tmp=Path(name);opds(tmp);p=tmp/'тест.pdf';p.write_bytes(pdf());scan(tmp,p)
        parts=[]
        for i in range(3):
            p=tmp/f'p{i}.ppm';p.write_bytes(b'P6\n80 100\n255\n'+bytes(v for y in range(100) for x in range(80) for v in ((x*3+i*40)%255,(y*2)%255,70)))
            dj=p.with_suffix('.djvu');subprocess.run(['c44',str(p),str(dj)],check=True);parts.append(dj)
        djvu=tmp/'скан.djvu';subprocess.run(['djvm','-c',str(djvu),*map(str,parts)],check=True)
        ocr=tmp/'page.txt';ocr.write_text('(page 0 0 80 100 (line 0 0 55 20 "Lighthouse atlas"))')
        subprocess.run(['djvused',str(djvu),'-s','-e',f'select 2; set-txt {ocr}'],check=True);scan(tmp,djvu)
    print('Проверены независимый OPDS с Basic Auth, разделы, картинки, пагинация, чтение EPUB; PDF и DjVu по три страницы, разворот и возврат на страницу 2.')
