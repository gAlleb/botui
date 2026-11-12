#!/usr/bin/env python3
"""PDF text selection, persisted ranges and shared editor in an isolated terminal."""
import importlib.util, json, os, tempfile, time, subprocess, xml.etree.ElementTree as ET
from pathlib import Path
spec=importlib.util.spec_from_file_location('shelf',Path(__file__).with_name('smoke-shelf.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
BINARY=Path(os.environ.get('BOTUI_BINARY',s.ROOT/'target/debug/botui')).resolve()

def pdf():
    objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Count 2 /Kids [3 0 R 5 0 R] >>']
    for page in range(2):
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {4+page*2} 0 R >>'.encode())
        content=b'BT /F1 18 Tf 30 340 Td (Alpha bravo charlie) Tj 0 -28 Td (Delta echo foxtrot) Tj 0 -60 Td (Another paragraph here) Tj ET'
        objects.append(f'<< /Length {len(content)} >>\nstream\n'.encode()+content+b'\nendstream')
    data=bytearray(b'%PDF-1.4\n');offsets=[0]
    for i,obj in enumerate(objects,1):
        offsets.append(len(data));data.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
    offset=len(data);data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for pos in offsets[1:]:data.extend(f'{pos:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{offset}\n%%EOF\n'.encode());return data

def notes(d):
    files=list((d/'state/botui/positions').glob('*.notes'))
    return json.loads(files[0].read_text()) if files else []

def copied(d,expected):
    for _ in range(100):
        if (d/'copied').exists() and (d/'copied').read_text()==expected:return
        time.sleep(.03)
    raise AssertionError((expected,(d/'copied').read_text() if (d/'copied').exists() else 'not copied'))

def start(d,file):
    return s.Terminal(d,Path('/usr/bin/env'),args=['TERM=xterm','COLORTERM=',str(BINARY),str(file)])

with tempfile.TemporaryDirectory(prefix='botui-pdf-text-') as tmp:
    d=Path(tmp);file=d/'words.pdf';file.write_bytes(pdf())
    (d/'config/botui').mkdir(parents=True)
    (d/'config/botui/config.toml').write_text('zen = true\nreading_layout = "single"\n')
    helpers=d/'bin';helpers.mkdir()
    for name in ('wl-copy','xclip','xsel','pbcopy'):
        p=helpers/name;p.write_text('#!/bin/sh\ncat > "$HOME/copied"\n');p.chmod(0o755)
    os.environ['PATH']=str(helpers)+os.pathsep+os.environ['PATH']
    t=start(d,file)
    try:
        frame=t.wait('PDF raster',lambda f:'▀' in f or '▄' in f)
        # Vim selection is opt-in; plain v must not change colors or enable a cursor.
        t.keys('v','V');assert 'VISUAL' not in t.run('capture-pane','-p','-t','shelf').stdout
        t.keys('Space','c','v');t.see('VISUAL')
        t.keys('Escape');t.see('NORMAL')
        t.keys('Space','c')
        t.wait('Space c leaves PDF text mode',lambda f:'Copy]' not in f and 'NORMAL' not in f and 'VISUAL' not in f)
        t.keys('Space','c','v');t.see('VISUAL');t.keys('Escape');t.see('NORMAL');t.keys('Escape')
        t.wait('Two Esc return to PDF reading',lambda f:'Copy]' not in f and 'NORMAL' not in f)
        t.keys('Space','c','v');t.see('VISUAL');t.keys('Space','c')
        t.wait('Space c leaves PDF Visual',lambda f:'Copy]' not in f and 'VISUAL' not in f)
        frame=t.wait('PDF raster restored',lambda f:'▀' in f or '▄' in f)
        # Infer the actual displayed raster rectangle, then click source word boxes.
        rows=[(i,[j for j,c in enumerate(line) if c in '▀▄']) for i,line in enumerate(frame.splitlines())]
        rows=[(i,x) for i,x in rows if len(x)>10]
        x0=min(min(x) for _,x in rows);x1=max(max(x) for _,x in rows)+1
        y0=rows[0][0];y1=rows[-1][0]+1
        xml=ET.fromstring(subprocess.check_output(['pdftotext','-f','1','-l','1','-bbox-layout',str(file),'-']))
        words={w.text:w for w in xml.iter() if w.tag.endswith('word')}
        def xy(word):
            a=words[word].attrib
            return (round(x0+(float(a['xMin'])+float(a['xMax']))/2/300*(x1-x0))+1,
                    int(y0+(float(a['yMin'])+float(a['yMax']))/2/400*(y1-y0))+1)
        def mouse(x,y,code=0,end='M'):
            seq=f'\033[<{code};{x};{y}{end}'
            t.run('send-keys','-t','shelf','-H',*[f'{b:02x}' for b in seq.encode()])
        # First right click loads the PDF text lazily and opens the same menu.
        mouse(*xy('bravo'),code=2);mouse(*xy('bravo'),code=2,end='m')
        frame=t.see('Add note')
        for label in ['Copy','Highlight','Search','Translate','Define','Add note']: assert label in frame,frame
        t.keys('Enter');copied(d,'bravo')
        mouse(*xy('bravo'));mouse(*xy('bravo'),end='m')
        t.wait('Mouse text ready',lambda f:'SELECT' in f and 'Loading text' not in f)
        t.keys('Space','a','y');copied(d,'bravo')
        # Reverse drag, with a UTF-8-safe source range (covered separately in unit tests).
        mouse(*xy('charlie'));mouse(*xy('Alpha'),code=32);mouse(*xy('Alpha'),end='m')
        t.keys('Space','a','y');copied(d,'Alpha bravo charlie')
        t.keys('Space','c');t.see('Copy]')
        # Wait for lazy text extraction without assuming a wall-clock duration.
        t.wait('Text layer ready',lambda f:'NORMAL' in f and 'Loading text' not in f)
        t.keys('y','i','w');copied(d,'Alpha')
        t.keys('w','v','w','y');copied(d,'bravo charlie')
        t.keys('v','b','Space','a','h');t.see('Highlight');t.keys('2','Enter')
        t.see('Annotation saved')
        data=notes(d);assert len(data)==1 and data[0]['color']=='green',data
        assert data[0]['quote']=='bravo charlie' and len(data[0]['pdf_regions'])==2,data
        assert data[0]['pdf_regions'][0]['page']==1
        time.sleep(.5)
        mouse(*xy('bravo'));mouse(*xy('bravo'),end='m')
        t.see('Comment');t.type('Remember this range');t.keys('C-s');t.see('Annotation saved')
        assert notes(d)[0]['body']=='Remember this range'
        # Reopen with no cached text: clicking the saved mark waits for extraction.
        t.run('kill-server',required=False)
        t=start(d,file)
        t.wait('Restored annotated PDF',lambda f:'▀' in f or '▄' in f)
        mouse(*xy('bravo'));mouse(*xy('bravo'),end='m')
        t.see('Remember this range');t.see('Ctrl+S Save');t.keys('Escape')
        # Clearing the comment preserves the highlight and its original identity.
        t.keys('Space','a','n');t.see('Comment');t.keys('C-u','C-s');t.see('Annotation saved')
        assert len(notes(d))==1 and notes(d)[0]['id']==data[0]['id'] and notes(d)[0]['body']==''
        # Removing the color as well deletes an empty annotation; choose it by mouse.
        t.keys('Space','a','n');frame=t.see('Comment')
        def click_label(label):
            frame=t.see(label)
            row,line=next((i,line) for i,line in enumerate(frame.splitlines(),1) if label in line)
            x=line.index(label)+1
            mouse(x,row);mouse(x,row,end='m')
        click_label('6 None');t.see('None ✓');click_label('[Ctrl+S Save]')
        t.see('Annotation deleted');assert notes(d)==[]
        t.keys('Space','a','h','Enter');t.see('Annotation saved');t.keys('Space','c')
        # Page rotation/zoom must not change stored coordinates or trigger a new extraction.
        before=notes(d);t.keys('Space','r','r','Space','r','+','Space','r','-');time.sleep(.2);assert notes(d)==before
        t.keys('Space','a','a');frame=t.see('PDF highlights & notes');assert '% · p.1/2' in frame,frame
        t.keys('n');t.see('Comment');t.type('Persisted PDF quote');t.keys('C-s')
        t.see('PDF highlights & notes');t.keys('e','Escape','Space','q')
        t.run('has-session','-t','shelf',required=False)
    finally:t.run('kill-server',required=False)
    assert 'Persisted PDF quote' in next((d/'.local/share/botui/exports').glob('*.md')).read_text()
    assert notes(d)[0]['pdf_regions']==before[0]['pdf_regions']
    t=start(d,file)
    try:
        t.wait('Restored PDF',lambda f:'▀' in f or '▄' in f)
        t.keys('Space','a','a');t.see('Persisted PDF quote')
        # A filtered export must not leak annotations hidden by the search.
        exports=d/'.local/share/botui/exports'
        existing=set(exports.glob('*.md'))
        t.keys('/');t.type('no-such-annotation');t.keys('Enter','E')
        t.see('This book has no notes to export')
        assert set(exports.glob('*.md'))==existing, 'Filtered export included hidden notes'
        t.keys('Escape','Space','a','a');t.see('Persisted PDF quote');t.keys('/')
        paste='\033[200~Persisted PDF quote\033[201~'
        t.run('send-keys','-t','shelf','-H',*[f'{b:02x}' for b in paste.encode()])
        t.see('Find: Persisted PDF quote');t.keys('Enter','End','Home','Y');copied(d,notes(d)[0]['quote'])
        t.keys('Delete');t.see('Delete annotation?');t.keys('Down','Enter');t.see('No annotations');assert notes(d)==[]
        t.keys('Escape','Space','q')
    finally:t.run('kill-server',required=False)
print('PASS: PDF ranges, copy, colors, editing, clearing, export and reopen')
