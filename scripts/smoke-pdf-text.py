#!/usr/bin/env python3
"""PDF text selection, persisted ranges and shared editor in an isolated terminal."""
import importlib.util, json, os, re, tempfile, time, subprocess, xml.etree.ElementTree as ET
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

def check_frame_hit(d, mode):
    """Locate the word from the drawn raster, independently of botui's hit map."""
    d.mkdir()
    file = d / 'frame-words.pdf'
    file.write_bytes(pdf())
    config = d / 'config/botui/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text('zen = true\nzen_progress = false\nborders = "rounded"\nreading_layout = "single"\n')
    xml = ET.fromstring(subprocess.check_output(['pdftotext', '-f', '1', '-l', '1', '-bbox-layout', str(file), '-']))
    word = next(w.attrib for w in xml.iter() if w.tag.endswith('word') and w.text == 'bravo')
    term = start(d, file)

    def framed(frame):
        return bool(re.fullmatch(rf'[╭┌] {re.escape(file.stem)} ─+[╮┐]', frame.partition('\n')[0]))

    def raster(frame, spread=False):
        # White pixels are rendered as white-background spaces. Measuring only
        # ▀/▄ would measure the ink, not the sheet, and miscalibrate scrolling.
        ansi = term.run('capture-pane', '-p', '-e', '-t', 'shelf').stdout
        points = []
        for y, line in enumerate(ansi.splitlines()):
            x, background = 0, None
            for token in re.finditer(r'\x1b\[([0-9;]*)m|([^\x1b]+)', line):
                if token[1] is not None:
                    codes = [int(v or 0) for v in token[1].split(';')]
                    i = 0
                    while i < len(codes):
                        code = codes[i]
                        if code in (0, 49):
                            background = None
                        elif code in (47, 107):
                            background = (255, 255, 255)
                        elif code in (38, 48) and codes[i + 1:i + 2] == [2]:
                            if code == 48:
                                background = tuple(codes[i + 2:i + 5])
                            i += 4
                        elif code in (38, 48) and codes[i + 1:i + 2] == [5]:
                            if code == 48:
                                background = None
                            i += 2
                        i += 1
                else:
                    if background == (255, 255, 255):
                        points.extend((col, y) for col in range(x, x + len(token[2]))
                                      if not spread or col < pane_width // 2)
                    x += len(token[2])
        assert points, frame
        return (min(x for x, _ in points), min(y for _, y in points),
                max(x for x, _ in points) + 1, max(y for _, y in points) + 1)

    def click_word(frame):
        x0, y0, x1, y1 = raster(frame, mode == 'spread')
        # In scroll mode the page continues below the viewport. Derive its full
        # height from the fitted raster's measured cell aspect, not the clipped edge.
        height = (x1 - x0) * page_ratio if mode == 'scroll' else y1 - y0
        x = round(x0 + (float(word['xMin']) + float(word['xMax'])) / 2 / 300 * (x1 - x0)) + 1
        y = int(y0 + (float(word['yMin']) + float(word['yMax'])) / 2 / 400 * height) + 1
        seq = f'\033[<0;{x};{y}M\033[<0;{x};{y}m'
        term.run('set-buffer', '-b', 'frame-mouse', seq)
        paste = ['paste-buffer', '-d', '-r', '-b', 'frame-mouse', '-t', 'shelf']
        if term.run(*paste, '-S', required=False).returncode:
            term.run(*paste)
        term.wait('Source word is selected on the rendered page', lambda f: 'SELECT' in f and 'Loading text' not in f)

    def copy_word():
        (d / 'copied').unlink(missing_ok=True)
        term.keys('Space', 'a', 'y')
        copied(d, 'bravo')
        term.keys('Escape')  # Copy cleared the range; Esc now closes the cursor.

    try:
        pane_width = int(term.run('display-message', '-p', '-t', 'shelf', '#{pane_width}').stdout)
        frame = term.wait('Fitted calibration page', lambda f: '▀' in f or '▄' in f)
        x0, y0, x1, y1 = raster(frame)
        page_ratio = (y1 - y0) / (x1 - x0)
        fitted_width = x1 - x0
        term.keys('Space', 'l', {'single': '1', 'spread': '2', 'scroll': '3'}[mode])
        def layout_ready(frame):
            if framed(frame) or not any(c in frame for c in '▀▄'):
                return False
            if mode == 'spread':
                mid = pane_width // 2
                return (any(c in row[:mid] for row in frame.splitlines() for c in '▀▄')
                        and any(c in row[mid:] for row in frame.splitlines() for c in '▀▄')
                        and not any(c in row[mid - 1:mid + 2] for row in frame.splitlines() for c in '▀▄'))
            if mode == 'scroll':
                left, _, right, _ = raster(frame)
                return right - left > fitted_width
            return True
        frame = term.wait('Unframed PDF layout', layout_ready)
        click_word(frame)
        term.keys('Space', 'p')
        term.wait('Selection survives the new frame', lambda f: framed(f) and 'SELECT' in f)
        copy_word()
        for visible in (True, False):
            if not visible:
                term.keys('Space', 'p')
            frame = term.wait('Raster ready for a fresh word hit', lambda f: framed(f) == visible
                              and 'SELECT' not in f and ('▀' in f or '▄' in f))
            click_word(frame)
            copy_word()
        term.keys('Space', 'q')
    finally:
        term.run('kill-server', required=False)

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
        mouse(*xy('charlie'))
        # Queue several sweeps before releasing at a different word. The copy
        # command following the release must see the release endpoint.
        x, y = xy('bravo')
        right, _ = xy('charlie')
        sweep = list(range(x, right + 1)) + list(range(right, x - 1, -1))
        burst = ''.join(f'\033[<32;{col};{y}M' for col in sweep * 8)
        left, row = xy('Alpha')
        burst += f'\033[<0;{left};{row}m'
        t.run('send-keys','-t','shelf','-l',burst)
        (d/'copied').unlink(missing_ok=True)
        t.keys('Space','a','y')
        for _ in range(100):
            quote=(d/'copied').read_text() if (d/'copied').exists() else ''
            if 'bravo' in quote: break
            time.sleep(.03)
        assert quote in 'Alpha bravo charlie' and 'bravo' in quote and quote[0] in 'Alpha', quote
        t.keys('Space','c');t.see('Copy]')
        # Wait for lazy text extraction without assuming a wall-clock duration.
        t.wait('Text layer ready',lambda f:'NORMAL' in f and 'Loading text' not in f)
        t.keys('y','i','w');copied(d,'Alpha')
        t.keys('0','w','v','$','y');copied(d,'bravo charlie')
        t.keys('g','v','Space','a','h');t.see('Highlight');t.keys('2','Enter')
        t.see('Annotation saved')
        data=notes(d);assert len(data)==1 and data[0]['color']=='green',data
        assert data[0]['quote']=='bravo charlie',data
        left, space, right = sorted(r['bounds'] for r in data[0]['pdf_regions'])
        assert left[2] == space[0] and space[2] == right[0], data
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
        click_label('6 No fill');t.see('No fill ✓');click_label('[Ctrl+S Save]')
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
    for mode in ('single', 'spread', 'scroll'):
        check_frame_hit(d / ('frame-' + mode), mode)
print('PASS: PDF ranges, copy, colors, editing, clearing, export, reopen and frame hit mapping')
