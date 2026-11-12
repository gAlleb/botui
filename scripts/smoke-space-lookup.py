#!/usr/bin/env python3
"""Space/Vim and lookup round trips, without desktop windows or personal services."""
import importlib.util, json, os, tempfile, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
BINARY=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui'))
requests=[]
feeds=[]
class API(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        feeds.append(self.headers.get('Authorization'))
        body=b'<feed xmlns="http://www.w3.org/2005/Atom"><id>catalog</id><title>Saved OPDS fixture</title><updated>2026-11-12T00:00:00Z</updated></feed>'
        self.send_response(200);self.send_header('Content-Type','application/atom+xml');self.end_headers();self.wfile.write(body)
    def do_POST(self):
        data=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(data)
        answer=json.dumps({'translatedText':f"Translated ({data['source']} -> {data['target']}): {data['q']}"}).encode()
        self.send_response(200);self.end_headers();self.wfile.write(answer)
server=ThreadingHTTPServer(('127.0.0.1',0),API)
threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='botui-space-') as d:
    p=Path(d);(p/'bin').mkdir();(p/'config/botui').mkdir(parents=True)
    for command in ['wl-copy','xclip','xsel','pbcopy']:
        f=p/'bin'/command;f.write_text('#!/bin/sh\ncat > "$HOME/copied"\n');f.chmod(0o755)
    os.environ['PATH']=str(p/'bin')+os.pathsep+os.environ['PATH']
    config=p/'config/botui/config.toml'
    config.write_text(f'''theme = "sepia-dark"
interface_theme = "catppuccin-mocha"
zen = true
leader_delay_ms = 80
[[opds]]
name = "Saved fixture"
url = "http://127.0.0.1:{server.server_port}/opds"
username = "reader"
password = "fixture-password"
[lookup]
dictionary = "Local definitions"
translation = "Test translation"
source_language = "auto"
target_language = "ru"
[[lookup.profiles]]
name = "Local definitions"
purpose = "dictionary"
provider = "command"
command = ["printf", "Definition: %s", "{{text}}"]
[[lookup.profiles]]
name = "Test translation"
purpose = "translation"
provider = "libre-translate"
endpoint = "http://127.0.0.1:{server.server_port}/translate"
''')
    book=p/'text.fb2';book.write_text('<FictionBook><body><section><p>Alpha bravo charlie delta. Привет мир.</p><p>Second paragraph for testing.</p></section></body></FictionBook>')
    t=s.Terminal(p,BINARY,args=[str(book)])
    try:
        t.see('Alpha')
        def book_theme():
            ansi = t.run('capture-pane','-p','-e','-t','shelf').stdout
            assert '38;2;255;213;149' in ansi, 'Book ink lost'
            assert '48;2;30;30;46' not in ansi, 'Interface paper leaked into reader popup'
            assert '38;2;205;214;244' not in ansi, 'Interface ink leaked into reader popup'
        book_theme()
        t.keys('Space','m');t.see('Reading')
        t.keys('v','t');t.see('Single page')
        t.keys('Space','l');t.see('Width & spacing')
        t.keys('Escape');t.see('Text cursor / Vim');t.keys('Escape')
        t.keys('Space');t.see('Text cursor / Vim');book_theme();t.keys('Escape')
        t.keys('Space','d');t.see('Word definitions');t.keys('q');t.see('Text cursor / Vim')
        t.keys('d');t.see('Word definitions');t.keys('Escape');t.see('Text cursor / Vim')
        t.keys('c');t.see('NORMAL')
        def copied(seq,expected):
            (p/'copied').unlink(missing_ok=True)
            t.keys(*seq)
            end=time.monotonic()+3
            while time.monotonic()<end:
                if (p/'copied').exists() and (p/'copied').read_text()==expected:break
                time.sleep(.03)
            else:raise AssertionError((seq,expected,(p/'copied').read_text() if (p/'copied').exists() else None))
        copied(['y','i','w'],'Alpha')
        t.keys('y','Escape');t.see('NORMAL')
        copied(['y','w'],'Alpha ')
        copied(['y','3','w'],'Alpha bravo charlie ')
        copied(['y','i','p'],'Alpha bravo charlie delta. Привет мир.')
        t.keys('Space','d','d');t.see('Definition: Alpha');book_theme()
        t.keys('Escape');t.see('NORMAL')
        t.keys('Space','d','t');t.see('Translated (auto -> ru): Alpha')
        assert requests[-1]['q']=='Alpha' and requests[-1]['target']=='ru',requests
        t.keys('l');t.see('English');t.keys('Home','Enter');t.see('Translated (auto -> en): Alpha')
        t.keys('n');t.see('Saved as a passage note')
        note=json.loads(next((p/'state/botui/positions').glob('*.notes')).read_text())[0]
        assert note['quote']=='Alpha' and 'Translated (auto -> en): Alpha' in note['body'],note
        t.keys('Space','a','h');t.see('Highlight');book_theme();t.keys('Escape')
        t.keys('Space','a','n');t.see('Comment');book_theme();t.keys('Escape')
        t.keys('Space','a','a');t.see('Highlights & notes');book_theme();t.keys('Escape')
        t.keys('v');t.see('VISUAL');t.keys('Escape');t.see('NORMAL')
        t.keys('Escape');t.wait('Esc returns to reading',lambda f:'NORMAL' not in f and 'Copy]' not in f)
        t.keys('v','V','t','b');assert 'VISUAL' not in t.run('capture-pane','-p','-t','shelf').stdout
        t.keys('Space','c','v');t.see('VISUAL');t.keys('Space','c')
        t.wait('Space c leaves Visual',lambda f:'NORMAL' not in f and 'Copy]' not in f)
        t.keys('F2');t.keys('Tab','Tab','Tab','Tab');t.see('Word definitions')
        t.keys('Down','Enter');t.see('Test translation')
        t.keys('End','Enter');t.see('Russian');t.keys('Home','Enter')
        t.see('Translate into: en')
        import tomllib
        saved=tomllib.loads(config.read_text())
        assert saved['lookup']['target_language']=='en' and len(saved['lookup']['profiles'])==2
        assert saved['opds'][0]['password']=='fixture-password' and saved['theme']=='sepia-dark'
        t.keys('Escape','Escape')
        print('PASS Space menu, Normal y/yiw/y3w/yip, Escape, dictionary, language switching, API request and saved note')
    finally:t.run("kill-server",required=False)
    t=s.Terminal(p,BINARY,args=['--opds',f'http://127.0.0.1:{server.server_port}/opds'])
    try:
        t.see('Saved OPDS fixture')
        import base64
        assert feeds[-1]=='Basic '+base64.b64encode(b'reader:fixture-password').decode(),feeds
        print('PASS saved OPDS URL and credentials from TOML')
    finally:t.run("kill-server",required=False)
server.shutdown()
