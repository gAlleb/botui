#!/usr/bin/env python3
"""A second device writes in the middle of a real TUI session. Its marker must survive."""
import importlib.util
from pathlib import Path
import tempfile
import threading
import time
spec=importlib.util.spec_from_file_location("shelf",Path(__file__).with_name("smoke-shelf.py"))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def run(name,f):
    t=s.Terminal(Path(name),s.ROOT/'target/debug/botui',url:=f'http://127.0.0.1:{f.server_port}/reader')
    try:
        s.login(t,url);t.keys('Enter')
        t.click('FB2',s.formats(t));t.see(s.FB2_MARK)
        t.keys('z');t.wait('Zen',lambda x:'Z Zen' not in x);t.keys('o');t.see('Chapters:');t.keys('End','Enter');t.wait('переход',lambda x:x.strip().startswith('Обратный рейс'));t.keys('q');s.formats(t)
        assert f.saved==[302],f.saved
        t.see("[H Put on hold]")
        # Erase only the isolated fixture's local position: the next jump must come from the API.
        previous,=(Path(name)/"state/botui/positions").glob("*.json")
        previous.unlink()
        f.marks[302]=[{"id":10,"title":"Last chapter on phone","cfi":f.progress[302]["text"]["cfi"]},{"id":11,"title":"Earlier edition","cfi":None}]
        t.click('FB2',s.formats(t));t.wait('возврат с сервера' ,lambda x:x.strip().startswith('Обратный рейс'))
        t.keys("g");t.see("Last chapter on phone");t.see("Earlier edition · unavailable in this file");t.keys("Enter")
        t.keys("b","g");t.see("Bookmarks: 1");assert [m["id"] for m in f.marks[302]]==[11];t.keys("Escape");t.wait("Closed bookmarks",lambda x:"Bookmarks:" not in x);t.keys("b","g");t.see("Bookmarks: 2")
        assert f.marks[302][-1]["cfi"]==f.progress[302]["text"]["cfi"]
        t.keys("Escape")
        # Same CFI, newer edit timestamp; policy is conservative.
        f.progress[302]['updatedAt']='2026-11-07T11:00:00.000Z'
        t.keys('o');t.see('Chapters:');t.keys('Home','Enter');t.wait('первая глава',lambda x:x.strip().startswith(s.FB2_MARK));t.keys('q');t.see('Another device updated')
        assert f.saved==[302] and f.progress[302]['updatedAt'].endswith('11:00:00.000Z')
        # An unsupported web position is never replaced by a local fraction.
        f.progress[302]['text']['cfi']='epubcfi(/6/999!/4/2/1:0)'
        t.click('FB2',s.formats(t));t.see(s.FB2_MARK);t.keys('o');t.see('Chapters:');t.keys('End','Enter')
        t.wait('переход без синхронизации',lambda x:x.strip().startswith('Обратный рейс'));t.keys('q')
        t.see('could not be located');assert f.saved==[302]
        # Closing the reader and immediately leaving the shelf must finish its pending save.
        f.progress[302]['text']['cfi']=f.marks[302][-1]['cfi']
        t.click('FB2',s.formats(t));t.see('Обратный рейс');t.keys('o');t.see('Chapters:');t.keys('Home','Enter')
        t.see(s.FB2_MARK);t.keys('q','q');t.see("Choose a library")
        assert f.saved==[302,302],f.saved
        before=sum(1 for method,path,status in f.requests if method=="POST" and path.endswith("/auth/login") and status==200)
        t.keys('b');t.see('Continue reading')
        assert sum(1 for method,path,status in f.requests if method=="POST" and path.endswith("/auth/login") and status==200)==before
        # Force the write to remain outstanding while Ctrl+C arrives. Logout must come afterwards.
        t.keys('1');t.see('All books');t.click(s.BOOK,t.see(s.BOOK));t.click('FB2',s.formats(t))
        t.see(s.FB2_MARK);t.keys('o');t.see('Chapters:');t.keys('End','Enter');t.see('Обратный рейс')
        f.save_entered=threading.Event();f.allow_save=threading.Event()
        t.keys('q');assert f.save_entered.wait(4);t.keys('C-c')
        try:
            t.see('Saving position to BookOrbit');assert f.token is not None
        finally:
            f.allow_save.set()
        for _ in range(110):
            if t.run('has-session','-t','shelf',required=False).returncode: break
            time.sleep(.04)
        assert f.saved==[302,302,302] and f.token is None
    finally:t.run('kill-server',required=False)
if __name__=='__main__':
    with tempfile.TemporaryDirectory() as tmp,s.Fixture(False) as f:
        threading.Thread(target=f.serve_forever,daemon=True).start()
        try:run(tmp,f)
        finally:f.shutdown()
    print('Проверены запись CFI для выбранного fileId, возврат из BookOrbit, конфликт и неизвестный CFI без перезаписи.')
