#!/usr/bin/env python3
"""Check overlapping text during a one-line move and two distinct raster pages in one viewport."""
from pathlib import Path
import importlib.util,tempfile,re,json,shutil,subprocess,time,os
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('sources',root/'scripts/smoke-sources.py');s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def start(d,path,*env):
    t=s.shelf.Terminal(d,Path('/usr/bin/env'),args=['NO_COLOR=1',*env,str(root/'target/debug/botui'),str(path)])
    return t
def stop(t):
    t.keys('q')
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
        t.see('Passage 001');t.keys('3');state=t.see('Scroll ·');first=re.findall(r'Passage \d+',state)
        t.keys('Down');second=t.wait('Line scroll keeps neighboring passages',lambda a:first[0] not in a and first[2] in a);assert len(set(first)&set(re.findall(r'Passage \d+',second)))>2
        t.keys('PageDown');last=t.wait('Page scroll advances',lambda a:first[2] not in a and 'Passage' in a)
        at=re.findall(r'Passage \d+',last)[0];t.keys('2','3');t.see(at);stop(t)
        assert saved(d)['scrolling']
        t=start(d,file);t.see(at)
        for _ in range(18):t.keys('PageDown')
        t.see('Passage 099');stop(t);assert saved(d)['finished']
    finally:t.run('kill-server',required=False)
def scan_scroll(d,file):
    d.mkdir();t=start(d,file)
    try:
        t.wait('First rendered page',lambda f:'::::::::' in f)
        t.keys('m');controls=t.see('Continuous scroll')
        assert controls.index('Single page') < controls.index('Two-page spread') < controls.index('Continuous scroll')
        t.click('Go to page number',controls);t.see('Go to page');t.keys('2','Enter');t.see('2 / 3');t.keys('b');t.see('Page bookmarked')
        t.keys('p');t.see('Go to page');t.keys('1','Enter');t.see('1 / 3')
        t.keys('m');t.click('Continuous scroll',t.see('Continuous scroll'));t.see('Scroll');both=False
        samples=[]
        for i in range(130):
            t.keys('Down');time.sleep(.04)
            a=t.run('capture-pane','-p','-t','shelf').stdout
            if i%8==0:samples.append([i, sorted(set(m.group(1) for m in re.finditer(r'([^\s])\1{6,}',a))),a.splitlines()[-1].strip()])
            if '::::::::' in a and '########' in a:
                both=True;break
        assert both,('No sheet transition',file.suffix,samples)
        # Jump away and back to the exact same slice spanning the two sheets.
        t.keys('p');t.see('Go to page');t.keys('3','Enter');t.see('3 / 3')
        t.keys(',');t.wait('Jump restores scroll offset',lambda a:'::::::::' in a and '########' in a)
        t.keys('.');t.see('3 / 3');t.keys(',');t.see('1 / 3')
        stop(t);state=saved(d);assert state['scrolling'] and state['scan_offset']>0
        t=start(d,file);t.wait('Restored within the same physical sheet',lambda a:'::::::::' in a and '########' in a)
        # Delete an off-screen bookmark without moving or losing the saved position.
        t.keys('g');t.see('Page bookmarks');t.keys('Delete');t.see('Delete bookmark?');t.keys('Escape')
        t.see('Page bookmarks');t.click('[×]',t.see('[×]'))
        t.click('[ Delete bookmark ]',t.see('[ Delete bookmark ]'))
        t.see('Bookmark removed');stop(t)
        after=saved(d)
        assert not after['marks'] and (after['block'],after['scan_offset'])==(state['block'],state['scan_offset'])
        t=start(d,file);t.see('1 / 3');t.keys('g');t.see('No entries yet.');t.keys('Escape')
        t.keys('2');t.wait('Spread leaves the scroll view',lambda a:'Scroll' not in a and '########' in a);stop(t)
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
        t.keys('2'); t.see('Page 2 unavailable')
        t.keys('1','/'); t.see('Find in text layer'); t.type('lighthouse'); t.keys('Enter')
        t.see('2 matching pages'); t.keys('F5'); frame=t.see('Matching pages')
        assert 'north shore' in frame and 'southern bay' in frame
        t.click('southern bay',frame); t.see('3 / 3')
        t.keys('F4'); t.see('1 / 3'); t.keys('F3'); t.see('3 / 3')
        t.keys('/'); t.see('Find in text layer'); t.keys('C-u'); t.type('north shore')
        t.keys('Enter'); t.see('1 matching page'); t.see('1 / 3')
        assert (bin_dir/'reads').read_text()=='x', 'A second query re-ran the document extractor'
        t.keys('p');t.see('Go to page');t.keys('2','Enter');t.see('Page 2 unavailable')
        t.keys('Right');t.see('3 / 3');stop(t)
        assert saved(d)['block']==2, 'The damaged page prevented saving the next readable page'
    finally: t.run('kill-server',required=False)
if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='botui-flow-') as tmp:
        d=Path(tmp);damaged_sheet_and_search(d/'mixed');text_scroll(d/'text');pdf=d/'pages.pdf';pdf.write_bytes(s.pdf(True));scan_scroll(d/'pdf',pdf)
        if shutil.which('c44') and shutil.which('djvm'):
            sheets=[]
            for i,color in enumerate((76,204,128)):
                ppm=d/f'{i}.ppm';ppm.write_bytes(b'P6\n120 175\n255\n'+bytes([color])*3*120*175)
                sheet=d/f'{i}.djvu';subprocess.run(['c44',str(ppm),str(sheet)],check=True);sheets.append(str(sheet))
            djvu=d/'pages.djvu';subprocess.run(['djvm','-c',str(djvu),*sheets],check=True);scan_scroll(d/'djvu',djvu)
    print('Passed line scrolling, reflow, completion and persisted cross-page PDF/DjVu scrolling.')
