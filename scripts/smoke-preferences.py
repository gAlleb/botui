#!/usr/bin/env python3
"""Open settings without a manual, operate with both a mouse and keys, and restart."""
from pathlib import Path
import importlib.util
import tempfile
import tomllib
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('shelf',root/'scripts/smoke-shelf.py')
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def check(tmp):
    t=s.Terminal(tmp,root/'target/debug/botui',args=[])
    def cfg():return tomllib.loads((tmp/'config/botui/config.toml').read_text())
    try:
        t.click('[? Help]',t.see('[? Help]'));t.see('Keyboard shortcuts');guide=t.see('Open the selected item')
        aligned=[next(row.index(label) for row in guide.splitlines() if label in row) for label in ('Open settings','Back / close a panel','Open the selected item')]
        assert len(set(aligned))==1,aligned
        assert 'series' in guide and 'Sections' in guide
        # Click the wide index and navigate back by section without losing the Keys tab.
        t.click('PDF & DjVu', guide)
        pdf = t.see('Go directly to a page number')
        assert '▸ PDF & DjVu' in pdf
        t.keys('[')
        t.see('Set page width and column gap')
        t.keys(']')
        t.see('Go directly to a page number')
        t.run('resize-window','-t','shelf','-x','42','-y','14')
        t.keys('End')
        tail=t.see('Book statistics')
        lines=tail.splitlines()
        last=next(i for i,row in enumerate(lines) if 'Book statistics' in row)
        assert 'close book' in ' '.join(row.strip(' │') for row in lines[last:last+3]),tail
        assert '▸ PDF & DjVu' not in tail
        t.keys('Home');t.see('Open settings')
        # Wheel over the body must advance physical rows, including wrapped descriptions.
        wheel=bytes.fromhex('1b')+b'[<65;20;7M'
        for _ in range(4):t.keys('-H',*[f'{b:02x}' for b in wheel])
        t.wait('Wheel moved past General',lambda f:'Open settings' not in f and 'Keyboard shortcuts' in f)
        t.keys('/')
        t.type('nonexistent-command')
        t.see('No matching shortcuts.')
        t.keys('C-u')
        t.type('Ctrl+G')
        compact = t.see('1 match')
        assert 'Go directly' in compact and 'to a page' in ' '.join(row.strip(' │') for row in compact.splitlines())
        t.keys('Enter')
        t.see('[Esc Clear]')
        t.keys('Escape')
        guide=t.see('Find a key or action')
        t.click('Find a key or action',guide);t.type('related books')
        t.see('1 match');t.click('[Esc Clear]',t.see('[Esc Clear]'));t.see('Open settings')
        t.run('resize-window','-t','shelf','-x','115','-y','40')
        # Wait for the wide layout itself; tmux can reflow the old 42-column
        # frame before BOTUI receives SIGWINCH and recalculates its hit boxes.
        t.wait('Wide shortcut index restored', lambda f: '▸ General' in f or 'Sections' in f and 'PDF & DjVu' in f)
        t.keys('Escape')
        home=t.wait('Wide source chooser',lambda f:'Inside this library' in f and '[F2 Settings]' in f)
        t.click('[F2 Settings]',home);t.see('Catppuccin Mocha');t.keys('Enter')
        t.see('Interface theme · preview');t.click('Gruvbox Dark',t.see('Gruvbox Dark'));t.click('[Esc Cancel]',t.see('[Esc Cancel]'));assert cfg()['interface_theme']=='catppuccin-mocha'
        t.keys('Enter');t.click('Nord',t.see('Nord'));t.keys('Enter')
        t.click('Background',t.see('Background'));t.see('Opaque')
        t.click('Borders & covers',t.see('Borders & covers'));t.click('Square',t.see('Square'))
        t.wait('Square borders applied',lambda f:'Solid terminal color' in f and cfg()['borders']=='square')
        t.keys('Tab');t.click('Column gap',t.see('Column gap'));t.keys('C-u');t.type('10');t.keys('Enter');t.see('10 cells')
        t.click('First-line indent',t.see('First-line indent'));t.keys('C-u');t.type('3');t.keys('Enter');t.see('3 cells')
        assert cfg()['paragraph_indent']==3
        t.keys('Tab');t.click('Reading statistics',t.see('Reading statistics'));t.see('Editions opened');t.keys('Escape')
        t.click('Download limit',t.see('4096 MiB'));t.keys('C-u');t.type('2048');t.keys('Enter');t.see('2048 MiB');assert cfg()['max_download_mib']==2048
        t.keys('Tab')
        t.click('Server', t.see('Password command'))
        t.see('BookOrbit server URL')
        t.type('invalid-address')
        t.see('invalid-address▏')
        t.keys('Enter')
        rejected = t.see('Invalid BookOrbit server address.')
        assert not cfg()['bookorbit']['server']
        t.click('Server', rejected)
        t.see('BookOrbit server URL')
        server = 'https://example.invalid'
        t.type(server)
        t.see(server + '▏')
        t.keys('Enter')
        account = t.wait('Saved server and returned to account settings',
                         lambda f: 'Password command' in f and server in f and cfg()['bookorbit']['server'] == server)
        t.click('Username', account)
        t.see('BookOrbit username')
        t.type('tester')
        t.see('tester▏')
        t.keys('Enter')
        account = t.wait('Saved username and returned to account settings',
                         lambda f: 'Password command' in f and 'tester' in f and cfg()['bookorbit']['username'] == 'tester')
        t.click('Password ', account)
        t.see('Replace saved password')
        t.type('masked-test')
        assert 'masked-test' not in t.see('•••••••••••▏')
        t.keys('Enter')
        t.see('Stored · replace or clear')
        t.keys('Escape');t.see('Your TUI book reader.');t.keys('q')
        data=cfg();assert (data['borders'],data['interface_theme'],data['column_gap'],data['opaque_background'])==('square','nord',10,True)
        assert data['bookorbit']['password']=='masked-test'
    finally:t.run('kill-server',required=False)
    t=s.Terminal(tmp,root/'target/debug/botui',args=[str(root/'examples/first-light.fb2')])
    try:
        t.see('Z Zen');t.keys('F2');t.see('Nord')
        t.keys('Escape','?');t.see('Keyboard shortcuts');t.see('EPUB & FB2');t.keys('Escape','q')
    finally:t.run('kill-server',required=False)
if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='botui-settings-') as d:check(Path(d))
    print('Passed themed settings, checked controls, masked password, mouse, independent palettes and restart.')
