#!/usr/bin/env python3
"""Use the actual reader and HTTP API for bidirectional annotation reconciliation."""
import importlib.util, json, os, threading, time
from pathlib import Path
import tempfile
from urllib.parse import urlsplit
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
BINARY=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()
BASE=s.API+'/books/200/annotations'
class Handler(s.Handler):
    def do_GET(self):
        if urlsplit(self.path).path==BASE:
            assert self.headers.get('Authorization')==f'Bearer {self.server.token}'
            return self.reply(503,{'message':'offline'}) if self.server.offline else self.reply(200,self.server.annotations)
        return super().do_GET()
    def do_POST(self):
        if self.path==BASE:
            body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            assert body['bookFileId'] in (301,303)
            assert (body.get('cfi') is not None) != (body.get('pdf') is not None)
            self.server.next_id+=1
            a=dict(id=self.server.next_id,cfi=body.get('cfi'),pdf=body.get('pdf'),jumpFileId=body['bookFileId'],text=body['text'],color=body['color'],style=body['style'],note=body['note'],updatedAt=None)
            self.server.annotations.append(a)
            return self.reply(201,a)
        return super().do_POST()
    def do_PATCH(self):
        if self.path.startswith(BASE+'/'):
            body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            assert set(body)=={'color','note'}
            a=next(a for a in self.server.annotations if a['id']==int(self.path.rsplit('/',1)[1]))
            a.update(body);return self.reply(200,a)
        return super().do_PATCH()
    def do_DELETE(self):
        if self.path.startswith(BASE+'/'):
            ident=int(self.path.rsplit('/',1)[1]);self.server.annotations[:]=[a for a in self.server.annotations if a['id']!=ident]
            return self.reply(204,b'',mime='text/plain')
        return super().do_DELETE()
def wait(label,predicate):
    end=time.monotonic()+25
    while time.monotonic()<end:
        if predicate():return
        time.sleep(.08)
    raise AssertionError(label)
def state(tmp):
    paths=list((tmp/'state/botui/positions').glob('*.notes'))
    if not paths:return None,None
    return paths[0],json.loads(paths[0].read_text())
def notes(tmp):
    _,v=state(tmp);return v.get('notes',[]) if isinstance(v,dict) else v or []
def sync_now(t):
    t.keys('Escape','Space','a','s');t.see('BookOrbit annotations')
    t.keys('Home','Down','Enter')
with tempfile.TemporaryDirectory(prefix='botui-annotation-sync-') as temp,s.Fixture(False) as server:
    tmp=Path(temp);server.RequestHandlerClass=Handler;server.annotations=[];server.next_id=0;server.offline=False
    threading.Thread(target=server.serve_forever,daemon=True).start()
    url=f'http://127.0.0.1:{server.server_port}/reader';t=s.Terminal(tmp,BINARY,url)
    try:
        s.login(t,url);t.keys('Enter');t.click('EPUB',s.formats(t));t.see(s.EPUB_MARK)
        # Select a word and save using the real annotation editor.
        frame=t.see('показаны');row,line=next((i,l) for i,l in enumerate(frame.splitlines(),1) if 'показаны' in l);x=line.index('показаны')+1
        seq=(f'\x1b[<0;{x};{row}M\x1b[<0;{x};{row}m')*2
        t.run('set-buffer','-b','mouse',seq);args=['paste-buffer','-d','-r','-b','mouse','-t','shelf']
        if t.run(*args,'-S',required=False).returncode:t.run(*args)
        t.see('SELECT');t.keys('Space','a','h');t.see('Highlight');t.keys('2','Enter');t.see('Annotation saved')
        wait('local highlight uploaded',lambda:len(server.annotations)==1)
        assert server.annotations[0]['text']=='показаны' and server.annotations[0]['color']=='#4ADE80',server.annotations
        server.annotations[0].update(note='Browser comment',color='#F472B6')
        sync_now(t);wait('browser comment imported',lambda:notes(tmp)[0]['body']=='Browser comment')
        assert notes(tmp)[0]['color']=='pink'
        t.keys('Space','a','a');t.see('Browser comment');t.keys('Escape')
        # Read-only HTTP failure must preserve the local note; edits while disconnected are durable.
        server.offline=True
        path,v=state(tmp);v['notes'][0]['body']='Offline edit';v['notes'][0]['updated']+=1;path.write_text(json.dumps(v))
        sync_now(t);t.see('Annotations pending locally');assert notes(tmp)[0]['body']=='Offline edit'
        server.offline=False;sync_now(t);wait('offline edit uploaded',lambda:server.annotations[0]['note']=='Offline edit')
        server.annotations.clear();sync_now(t);wait('remote removal imported',lambda:not notes(tmp))
        t.keys('Space','q');s.formats(t)
        assert any(r[0]=='PATCH' and r[1].startswith(BASE) for r in server.requests)
        server.progress[303]=dict(fileId=303,updatedAt=None,percentage=33,text={"cfi":None,"page":2})
        t.click('PDF',s.formats(t));t.wait('PDF page opened',lambda f:'2 / 3' in f and 'Space r p' in f)
        t.keys('Space','c');t.wait('PDF text ready',lambda f:'NORMAL' in f and 'Loading text' not in f)
        t.keys('v','Space','a','h');t.see('Highlight');t.keys('3','Enter');t.see('Annotation saved')
        wait('PDF highlight uploaded',lambda:len(server.annotations)==1)
        a=server.annotations[0];assert a['pdf']['page']==1 and a['cfi'] is None and a['text']=='Lighthouse',a
        assert a['pdf']['rect']['width']>0 and a['pdf']['rect']['height']>0
        a['note']='PDF browser comment';sync_now(t)
        def pdf_notes():
            for p in (tmp/'state/botui/positions').glob('*.notes'):
                v=json.loads(p.read_text());ns=v.get('notes',[]) if isinstance(v,dict) else v
                if any(n.get('pdf_regions') for n in ns):return ns
            return []
        wait('PDF browser comment imported',lambda:any(n['body']=='PDF browser comment' for n in pdf_notes()))
        t.keys('Space','a','a');t.see('Highlights & notes');t.see('PDF browser comment')
        t.keys('Escape','Escape');t.wait('PDF note list closed',lambda f:'Highlights & notes' not in f and 'Space r p' in f)
        t.keys('Space','q');s.formats(t)
        print('PASS reader selection → BookOrbit, browser comment/color → reader, offline retry, remote delete and PDF geometry/comment round trip')
    finally:t.run('kill-server',required=False)
