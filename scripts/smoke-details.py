#!/usr/bin/env python3
"""Visit a native book without starting it, pause it, then cancel and confirm a reset."""
from pathlib import Path
import importlib.util, tempfile, threading, tomllib
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("shelf",root/"scripts/smoke-shelf.py");s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def exercise(tmp, f):
    url=f"http://127.0.0.1:{f.server_port}/reader"
    t=s.Terminal(tmp,root/"target/debug/botui",url)
    try:
        s.login(t,url);t.run("resize-window","-t","shelf","-x","148","-y","38")
        frame=t.wait("Expanded Libraries",lambda a:"▾ 4 Libraries" in a and s.LIBRARIES[32] in a)
        t.click("4 Libraries",frame);t.wait("Click folds tree without changing the shelf",lambda a:"▸ 4 Libraries" in a and s.LIBRARIES[32] not in a and s.BOOK in a)
        t.keys("Right");t.see(s.LIBRARIES[32]);t.keys("Left","F6");t.see("▸ 4 Libraries")
        assert tomllib.loads((tmp/"config/botui/config.toml").read_text())["bookorbit"]["libraries_collapsed"]
        t.keys("2");t.click(s.OTHER_BOOK,t.see(s.OTHER_BOOK))
        t.see("Harbor Press");t.see("Beacon #2");t.see("37% read");t.see("northern lighthouse")
        t.click("[I Synopsis]",t.see("[I Synopsis]"));t.see("[ Close ]");t.keys("Escape")
        assert not f.saved and not any(path.endswith('/serve') for _,path,_ in f.requests)
        t.click("[H Put on hold]",t.see("[H Put on hold]"));t.see("Reading status: On hold")
        t.keys("Escape");t.wait("Paused title is gone from Currently reading",lambda a:"Currently reading" in a and s.OTHER_BOOK not in a)
        t.keys("1");t.click(s.OTHER_BOOK,t.see(s.OTHER_BOOK));t.see("[H Resume reading]");t.keys("h");t.see("Reading status: Reading")
        t.keys("x");t.see("cannot be undone.");t.keys("Enter");t.see("Editions");assert f.statuses[201]=="reading"
        t.keys("x");t.see("cannot be undone.");t.keys("Right","Enter");t.see("Reading state reset.");assert f.statuses[201]=="unread"
        t.keys("2");t.wait("Reset title absent",lambda a:"Currently reading" in a and s.OTHER_BOOK not in a)
        assert len([v for v in f.requests if v[1].endswith('/reset-reading-state')])==1
        t.keys("q");t.see("Choose a library");t.keys("q")
    finally:t.run("kill-server",required=False)
if __name__=="__main__":
    with tempfile.TemporaryDirectory(prefix="botui-card-") as d,s.Fixture(False) as f:
        th=threading.Thread(target=f.serve_forever,daemon=True);th.start()
        try:exercise(Path(d),f)
        finally:f.shutdown();th.join()
    print("Passed folding navigation, native edition details, pause/resume and explicit reset without opening a book.")
