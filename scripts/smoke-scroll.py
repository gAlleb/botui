#!/usr/bin/env python3
"""Check overlapping text during a one-line move and two distinct raster pages in one viewport."""
from pathlib import Path
import importlib.util,tempfile,re,json,shutil,subprocess,time,os
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('sources',root/'scripts/smoke-sources.py');s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def start(d,path,*env):
    t=s.shelf.Terminal(d,Path('/usr/bin/env'),args=['NO_COLOR=1',*env,str(Path(os.environ.get('BOTUI_BINARY', root/'target/debug/botui')).resolve()),str(path)])
    return t
def stop(t):
    t.keys('Space','q')
    for _ in range(100):
        if t.run('has-session','-t','shelf',required=False).returncode:break
        time.sleep(.035)
    else:raise AssertionError('reader failed to exit')
    t.run('kill-server',required=False)
def saved(d):
    for p in d.rglob('*.json'):
        val=json.loads(p.read_text())
        if isinstance(val,dict) and 'spread' in val and 'block' in val:return val
def text_scroll(d):
    d.mkdir();file=d/'scroll.fb2'
    file.write_text('<FictionBook><body><section>'+''.join(f'<p>Passage {i:03}.</p>' for i in range(100))+'</section></body></FictionBook>')
    t=start(d,file)
    try:
        t.see('Passage 001');t.keys('Space', 'l', '3');state=t.see('Scroll ·');first=re.findall(r'Passage \d+',state)
        t.keys('Down');second=t.wait('Line scroll keeps neighboring passages',lambda a:first[0] not in a and first[2] in a);assert len(set(first)&set(re.findall(r'Passage \d+',second)))>2
        t.keys('Up');t.wait('Up restores first row',lambda a: re.findall(r'Passage \d+',a)==first)
        wheel(t,65);t.wait('Wheel defaults to exactly one line',lambda a:a==second)
        t.keys('PageDown');last=t.wait('Page scroll advances',lambda a:first[2] not in a and 'Passage' in a)
        at=re.findall(r'Passage \d+',last)[0];t.keys('Space', 'l', '2', 'Space', 'l', '3');t.see(at);stop(t)
        assert saved(d)['scrolling']
        t=start(d,file);t.see(at)
        for _ in range(18):t.keys('PageDown')
        t.see('Passage 099');stop(t);assert saved(d)['finished']
    finally:t.run('kill-server',required=False)

def wheel(t,button):
    data=f'\033[<{button};70;12M'
    t.run('send-keys','-t','shelf','-H',*[f'{b:02x}' for b in data.encode()])

def configured_wheel(d):
    d.mkdir();file=d/'scroll.fb2'
    file.write_text('<FictionBook><body><section>'+''.join(f'<p>Wheel passage {i:03}.</p>' for i in range(100))+'</section></body></FictionBook>')
    (d/'config/botui').mkdir(parents=True)
    (d/'config/botui/config.toml').write_text('scroll_lines = 3\n')
    t=start(d,file)
    try:
        t.see('Wheel passage');t.keys('Space', 'l', '3');first=t.see('Scroll')
        t.keys('Down','Down','Down');third=t.wait('Three keyboard lines',lambda f:'Wheel passage 002' in f and f!=first)
        t.keys('Up','Up','Up');t.wait('Return to starting slice',lambda f:f==first)
        wheel(t,65);t.wait('Configured wheel equals three single lines',lambda f:f==third)
        t.keys('F2');t.see('Appearance');t.click('Reader',t.see('Appearance'))
        frame=t.see('Mouse wheel step');t.click('Mouse wheel step',frame)
        t.see('1–12');t.keys('C-u','1','Enter');t.see('Mouse wheel step');t.keys('Escape');t.see('Scroll')
        before=t.run('capture-pane','-p','-t','shelf').stdout
        t.keys('Down');after=t.wait('One arrow after settings',lambda f:f!=before and 'Scroll' in f)
        t.keys('Up');t.wait('Arrow reverse',lambda f:f==before)
        wheel(t,65);t.wait('Setting takes effect in the open book',lambda f:f==after)
    finally:t.run('kill-server',required=False)
def scan_scroll(d,file):
    d.mkdir();t=start(d,file)
    try:
        t.wait('First rendered page',lambda f:'::::::::' in f)
        t.keys('Space', 'm');controls=t.see('Continuous scroll')
        assert controls.index('Single page') < controls.index('Two-page spread') < controls.index('Continuous scroll')
        t.click('Go to page number',controls);t.see('Go to page');t.keys('2','Enter');t.see('2 / 3');t.keys('Space', 'b', 'a');t.see('Page bookmarked')
        t.keys('Space', 'r', 'p');t.see('Go to page');t.keys('1','Enter');t.see('1 / 3')
        t.keys('Space', 'm');t.click('Continuous scroll',t.see('Continuous scroll'));t.see('Scroll');both=False
        samples=[]
        for i in range(130):
            t.keys('Down');time.sleep(.04)
            a=t.run('capture-pane','-p','-t','shelf').stdout
            if i%8==0:samples.append([i, sorted(set(m.group(1) for m in re.finditer(r'([^\s])\1{6,}',a))),a.splitlines()[-1].strip()])
            if '::::::::' in a and '########' in a:
                both=True;break
        assert both,('No sheet transition',file.suffix,samples)
        assert '········' in a, 'No dotted separator between physical sheets'
        t.keys('Down');one=t.wait('One scan line',lambda f:f!=a and '········' in f)
        t.keys('Up');t.wait('Scan line restored',lambda f:f==a)
        wheel(t,65);t.wait('PDF/DjVu wheel equals one scan line',lambda f:f==one)
        t.keys('Up');t.wait('Transition restored',lambda f:f==a)
        # Jump away and back to the exact same slice spanning the two sheets.
        t.keys('Space', 'r', 'p');t.see('Go to page');t.keys('3','Enter');t.see('3 / 3')
        t.keys('Space', 'r', '[');t.wait('Jump restores scroll offset',lambda a:'::::::::' in a and '########' in a)
        t.keys('Space', 'r', ']');t.see('3 / 3');t.keys('Space', 'r', '[');t.see('1 / 3')
        stop(t);state=saved(d);assert state['scrolling'] and state['scan_offset']>0
        t=start(d,file);t.wait('Restored within the same physical sheet',lambda a:'::::::::' in a and '########' in a)
        # Delete an off-screen bookmark without moving or losing the saved position.
        t.keys('Space', 'b', 'l');t.see('Page bookmarks');t.keys('Delete');t.see('Delete bookmark?');t.keys('Escape')
        t.see('Page bookmarks');t.click('[×]',t.see('[×]'))
        t.click('[ Delete bookmark ]',t.see('[ Delete bookmark ]'))
        t.see('Bookmark removed');stop(t)
        after=saved(d)
        assert not after['marks'] and (after['block'],after['scan_offset'])==(state['block'],state['scan_offset'])
        t=start(d,file);t.see('1 / 3');t.keys('Space', 'b', 'l');t.see('No entries yet.');t.keys('Escape')
        t.keys('Space', 'l', '2');t.wait('Spread leaves the scroll view',lambda a:'Scroll' not in a and '########' in a);stop(t)
    finally:t.run('kill-server',required=False)
def damaged_sheet_and_search(d):
    # Force a real renderer subprocess failure on page 2, and count text extractions.
    d.mkdir()
    path=d/'mixed.pdf'
    path.write_bytes(s.pdf())
    bin_dir=d/'bin'
    bin_dir.mkdir()
    program=bin_dir/'pdftoppm'
    program.write_text("""#!/usr/bin/env python3
import sys
page=int(sys.argv[sys.argv.index('-f')+1])
if page==2: sys.exit(1)
sys.stdout.buffer.write(b'P6\\n20 30\\n255\\n'+bytes([76])*1800)
""")
    program.chmod(0o755)
    program=bin_dir/'pdftotext'
    program.write_text("""#!/usr/bin/env python3
import sys
from pathlib import Path
with (Path(__file__).parent / 'reads').open('a') as log: log.write('x')
sys.stdout.write('Lighthouse on the north shore\\f\\fLighthouse by the southern bay\\f')
""")
    program.chmod(0o755)
    t=start(d,path,'PATH='+str(bin_dir)+os.pathsep+os.environ['PATH'])
    try:
        t.see('1 / 3')
        t.keys('Space', 'l', '2'); t.see('Page 2 unavailable')
        t.keys('Space', 'l', '1', 'Space', 'f'); t.see('Find in text layer'); t.type('lighthouse'); t.keys('Enter')
        t.see('2 matching pages'); t.keys('F5'); frame=t.see('Matching pages')
        assert 'north shore' in frame and 'southern bay' in frame
        t.click('southern bay',frame); t.see('3 / 3')
        t.keys('F4'); t.see('1 / 3'); t.keys('F3'); t.see('3 / 3')
        t.keys('Space', 'f'); t.see('Find in text layer'); t.keys('C-u'); t.type('north shore')
        t.keys('Enter'); t.see('1 matching page'); t.see('1 / 3')
        assert (bin_dir/'reads').read_text()=='x', 'A second query re-ran the document extractor'
        t.keys('Space', 'r', 'p');t.see('Go to page');t.keys('2','Enter');t.see('Page 2 unavailable')
        t.keys('Right');t.see('3 / 3');stop(t)
        assert saved(d)['block']==2, 'The damaged page prevented saving the next readable page'
    finally: t.run('kill-server',required=False)
def slow_jump(d):
    # Hold a distant page in the renderer; the old sheet must remain readable.
    d.mkdir()
    book=d/'nine.pdf'
    book.write_bytes(s.pdf(True,300,440,(.3,)*9))
    bins=d/'bin';bins.mkdir()
    renderer=bins/'pdftoppm'
    renderer.write_text("""#!/usr/bin/env python3
import sys,time
from pathlib import Path
d=Path(__file__).parent
n=int(sys.argv[sys.argv.index('-f')+1])
with (d/'calls').open('a') as f: print(n,file=f)
if n==8:
    while not (d/'release').exists(): time.sleep(.01)
value={1:76,7:173,8:205}.get(n,110)
sys.stdout.buffer.write(b'P6\\n20 30\\n255\\n'+bytes([value])*1800)
""")
    renderer.chmod(0o755)
    t=start(d,book,'PATH='+str(bins)+os.pathsep+os.environ['PATH'])
    try:
        t.see('::::::::')
        t.keys('Space', 'r', 'p');t.see('Go to page');t.keys('8','Enter')
        t.wait('Visible sheet stays on screen until the destination arrives',
               lambda f: '→ 8' in f and '::::::::' in f and 'Rendering page' not in f)
        (bins/'release').touch()
        t.wait('New sheet replaces old sheet',lambda f: '8 / 9' in f and '########' in f)
        t.keys('Left');t.wait('Reverse page', lambda f:'7 / 9' in f and '********' in f)
        t.keys('Right');t.see('########')
        calls=(bins/'calls').read_text().splitlines()
        assert calls.count('8')==1,calls
    finally:t.run('kill-server',required=False)

if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='botui-flow-') as tmp:
        d=Path(tmp);slow_jump(d/'jump');damaged_sheet_and_search(d/'mixed');text_scroll(d/'text');configured_wheel(d/'wheel');pdf=d/'pages.pdf';pdf.write_bytes(s.pdf(True));scan_scroll(d/'pdf',pdf)
        if shutil.which('c44') and shutil.which('djvm'):
            sheets=[]
            for i,color in enumerate((76,204,128)):
                ppm=d/f'{i}.ppm';ppm.write_bytes(b'P6\n120 175\n255\n'+bytes([color])*3*120*175)
                sheet=d/f'{i}.djvu';subprocess.run(['c44',str(ppm),str(sheet)],check=True);sheets.append(str(sheet))
            djvu=d/'pages.djvu';subprocess.run(['djvm','-c',str(djvu),*sheets],check=True);scan_scroll(d/'djvu',djvu)
    print('Passed line scrolling, reflow, completion and persisted cross-page PDF/DjVu scrolling.')
