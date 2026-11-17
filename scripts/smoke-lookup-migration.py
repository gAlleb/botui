#!/usr/bin/env python3
"""Existing default connections migrate to generic HTTP and retain working requests."""
import importlib.util, json, os, tempfile, threading, tomllib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse, parse_qs
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
binary=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()
requests=[]
class API(BaseHTTPRequestHandler):
    def do_GET(self):
        requests.append(self.path)
        if self.path.startswith('/definitions/'):
            body=[{'meanings':[{'definitions':[{'definition':'Dictionary fixture'}]}]}]
        else:
            q=parse_qs(urlparse(self.path).query)
            assert q['sl']==['auto'] and q['tl']==['ru'] and q['q']==['Alpha'],q
            body=[[['Перевод ', 'Alpha'],['фикстуры', '']]]
        data=json.dumps(body).encode();self.send_response(200);self.end_headers();self.wfile.write(data)
    def log_message(self,*_): pass
server=ThreadingHTTPServer(('127.0.0.1',0),API)
threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 with tempfile.TemporaryDirectory(prefix='botui-lookup-migration-') as tmp:
    root=Path(tmp);config=root/'config/botui/config.toml';config.parent.mkdir(parents=True)
    config.write_text(f'''# Keep personal settings
[lookup]
dictionary = "English dictionary"
translation = "Google Translate"
source_language = "auto"
target_language = "ru"
[[lookup.profiles]]
name = "English dictionary"
purpose = "dictionary"
provider = "free-dictionary"
endpoint = "http://127.0.0.1:{server.server_port}/definitions"
[[lookup.profiles]]
name = "Google Translate"
purpose = "translation"
provider = "google-web"
endpoint = "http://127.0.0.1:{server.server_port}/translate"
''')
    book=root/'book.fb2';book.write_text('<FictionBook><body><section><p>Alpha bravo.</p></section></body></FictionBook>')
    t=s.Terminal(root,binary,args=[str(book)])
    try:
        t.see('Alpha bravo.');assert not requests
        c=tomllib.loads(config.read_text());assert all(p['provider']=='http' for p in c['lookup']['profiles'])
        assert c['lookup']['dictionary']=='English dictionary' and c['lookup']['translation']=='Google Translate'
        assert '# Keep personal settings' in config.read_text()
        t.keys('Space','c','Space','d','d');frame=t.see('Dictionary fixture')
        assert 'Not used by profile' not in frame and 'Target language' not in frame and 'Source language' not in frame,frame
        assert 'auto → ru' not in frame,frame
        t.keys('l');t.see('does not use a target-language');assert len(requests)==1
        t.click('R Retry',t.see('Dictionary fixture'))
        t.wait('Retry uses the visible button action',lambda f:len(requests)==2 and 'Dictionary fixture' in f)
        t.keys('Escape');t.wait('Definition window closed',lambda f:'Dictionary fixture' not in f)
        t.keys('Space','d','t');frame=t.see('Перевод фикстуры');assert len(requests)==3
        assert 'Target language' in frame and 'Source language' in frame and 'auto → ru' in frame,frame
        t.keys('Escape','Space','q')
    finally:t.run('kill-server',required=False)
finally:server.shutdown();server.server_close()
print('PASS default dictionary/translation migration, saved HTTP fields, lazy requests and language controls')
