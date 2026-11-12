#!/usr/bin/env python3
"""Dictionary CRUD, generic HTTP requests, layout and nested settings palettes."""
import importlib.util
import json
import os
import tempfile
import threading
import tomllib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
binary = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui')).resolve()
requests = []
class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append((body, self.headers.get('Authorization')))
        response = json.dumps({'choices': [{'message': {'content': 'My explanation'}}]} if 'messages' in body else {'items': [{'meaning': 'Custom definition: ' + body['word']}]}).encode()
        self.send_response(200); self.end_headers(); self.wfile.write(response)
    def log_message(self, *_): pass
server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
 with tempfile.TemporaryDirectory(prefix='botui-lookup-settings-') as tmp:
    root = Path(tmp)
    config = root / 'config/botui/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text('''theme = "sepia-dark"
interface_theme = "catppuccin-mocha"
opaque_background = true
leader_delay_ms = 150
[lookup]
profiles = []
[[opds]]
name = "Untouched catalog"
url = "https://example.org/opds"
''')
    book = root / 'sample.fb2'
    book.write_text('<FictionBook><body><section><p>Hello book.</p></section></body></FictionBook>')
    t = s.Terminal(root, binary, args=[str(book)])
    def click(label):
        frame = t.see(label); t.click(label, frame)
    def replace(value):
        t.keys('C-u'); t.type(value); t.keys('Enter')
    def data(): return tomllib.loads(config.read_text())
    def palette():
        ansi = t.run('capture-pane', '-p', '-e', '-t', 'shelf').stdout
        assert '38;2;255;213;149' not in ansi, 'Book theme leaked into settings'
        assert '48;2;30;30;46' in ansi, 'Interface background missing'
    def settings():
        t.see('Hello book.'); t.keys('F2', 'Tab', 'Tab', 'Tab', 'Tab')
        frame = t.see('Word definitions')
        lines = [line for line in frame.splitlines() if 'connections' in line]
        assert len(lines) == 2 and lines[0].index('0 connections') == lines[1].index('0 connections'), frame
        for line in lines:
            assert line.index(line.lstrip('│╭╰ ').split()[0]) >= 2, line
        palette()
    try:
        settings(); click('Word definitions'); click('+ Add dictionary')
        frame = t.see('connection type')
        assert 'HTTP API' in frame and 'OpenAI-compatible API' in frame and 'Local command' in frame
        assert 'Google' not in frame and 'DeepL' not in frame
        palette(); t.keys('Enter'); t.see('Connection type')
        click('Name'); replace('My HTTP dictionary')
        click('Description'); palette(); replace('My own optional description')
        t.see('My own optional description')
        click('Server'); replace(f'http://127.0.0.1:{server.server_port}/lookup')
        click('Method'); click('POST')
        click('JSON body'); replace('{"word":"{text}","language":"{target}"}')
        click('Text path'); replace('/items/*/meaning')
        click('Headers'); click('+ Add'); replace('Authorization'); replace('Bearer {key}'); click('Apply')
        click('API key'); replace('fixture-only-secret')
        frame = t.see('Saved (hidden)'); assert 'fixture-only-secret' not in frame
        t.keys('End'); click('Test connection with')
        t.see('Custom definition: hello'); palette(); t.keys('Escape'); t.see('Connection type'); t.keys('End')
        click('Save changes & back'); t.see('My HTTP dictionary')
        p = data()['lookup']['profiles'][0]
        assert p['provider'] == 'http' and p['description'] == 'My own optional description'
        assert p['headers']['Authorization'] == 'Bearer {key}'
        assert requests == [({'word': 'hello', 'language': 'en'}, 'Bearer fixture-only-secret')], requests
        click('My HTTP dictionary'); t.keys('Home'); click('Description'); replace('')
        frame = t.see('Description'); assert 'My own optional description' not in frame
        # A small window scrolls the fields; bottom actions remain reachable.
        t.run('resize-window', '-t', 'shelf', '-x', '54', '-y', '18')
        t.keys('End'); click('Save changes & back'); t.see('+ Add dictionary')
        t.run('resize-window', '-t', 'shelf', '-x', '115', '-y', '30')
        click('My HTTP dictionary'); t.keys('Home'); click('Name'); replace('Discarded name')
        t.keys('Escape'); click('Discard changes')
        assert data()['lookup']['profiles'][0]['name'] == 'My HTTP dictionary'
        click('My HTTP dictionary'); t.keys('End'); click('Delete connection')
        t.keys('Down', 'Enter'); t.see('+ Add dictionary')
        assert data()['lookup']['profiles'] == []
        click('+ Add dictionary'); click('OpenAI-compatible API')
        click('Name'); replace('My explanation service')
        click('Server'); replace(f'http://127.0.0.1:{server.server_port}/chat')
        click('Model'); replace('fixture-model')
        click('Explanation language'); click('Russian')
        t.keys('End'); click('Test connection with'); t.see('My explanation'); palette()
        assert 'in ru.' in requests[-1][0]['messages'][0]['content'], requests[-1]
        t.keys('Escape'); t.see('Connection type'); t.keys('End'); click('Save changes & back'); t.see('+ Add dictionary')
        assert data()['lookup']['profiles'][0]['target_language'] == 'ru'
        click('My explanation service'); t.keys('End'); click('Delete connection'); t.keys('Down', 'Enter')
        t.see('+ Add dictionary'); assert data()['lookup']['profiles'] == []
        assert data()['opds'][0]['name'] == 'Untouched catalog'
        t.keys('Escape'); t.see('0 connections'); t.keys('Escape'); t.see('Hello book.')
        ansi = t.run('capture-pane', '-p', '-e', '-t', 'shelf').stdout
        assert '38;2;255;213;149' in ansi, 'Reader palette was not restored'
    finally:
        t.run('kill-server', required=False)
    # Empty lists persist across a full process restart.
    t = s.Terminal(root, binary, args=[str(book)])
    try:
        settings(); click('Word definitions'); frame = t.see('+ Add dictionary')
        assert 'My HTTP dictionary' not in frame and 'Google' not in frame
        print('PASS dictionary CRUD, empty restart, optional descriptions, HTTP JSON request/response, masked headers, aligned counts, narrow/mouse layout and F2 palette restoration')
    finally:
        t.run('kill-server', required=False)
finally:
 server.shutdown(); server.server_close()
