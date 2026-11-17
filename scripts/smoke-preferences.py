#!/usr/bin/env python3
"""Open settings without a manual, operate with both a mouse and keys, and restart."""
from pathlib import Path
import importlib.util
import re
import tempfile
import tomllib
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('shelf',root/'scripts/smoke-shelf.py')
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
def check(tmp):
    t=s.Terminal(tmp,root/'target/debug/botui',args=[])
    def cfg():return tomllib.loads((tmp/'config/botui/config.toml').read_text())
    def spacing_saved(compact, gap):
        data = cfg()
        return data.get('preserve_compact_spacing', True) == compact and data.get('paragraph_spacing') == gap
    def compact_row(frame, enabled):
        return re.search(r'Compact blocks\s+' + ('On' if enabled else 'Off') + r'\b', frame)
    try:
        help_button = '[F1/? Help]'
        home_footer = f'[F2 Settings] {help_button}   ↑↓ Choose   [Enter Open] [Q Quit]'
        for key in ('?', 'F1'):
            t.see(home_footer)
            t.keys(key)
            t.see('Keyboard shortcuts')
            t.keys('Escape')
        t.click(help_button, t.see(home_footer));guide=t.see('Open the selected item')
        aligned=[next(row.index(label) for row in guide.splitlines() if label in row) for label in ('Open settings','Back / close a panel','Open the selected item')]
        assert len(set(aligned))==1,aligned
        assert 'series' in guide and 'Sections' in guide
        # Click the wide index and navigate back by section without losing the Keys tab.
        t.click('PDF, DjVu & comics', guide)
        pdf = t.see('Navigate pages; arrows pan')
        assert '▸ PDF, DjVu & comics' in pdf
        t.keys('[')
        t.see('Navigate; scroll mode')
        t.keys(']')
        t.see('Navigate pages; arrows pan')
        t.run('resize-window','-t','shelf','-x','42','-y','14')
        t.keys('End')
        tail=t.see('column gap')
        assert 'Page layout' in tail
        assert '▸ PDF, DjVu & comics' not in tail
        t.keys('Home');t.see('Open settings')
        # Wheel over the body must advance physical rows, including wrapped descriptions.
        wheel=bytes.fromhex('1b')+b'[<65;20;7M'
        for _ in range(4):t.keys('-H',*[f'{b:02x}' for b in wheel])
        t.wait('Wheel moved past General',lambda f:'Open settings' not in f and 'Keyboard shortcuts' in f)
        t.keys('/')
        t.type('nonexistent-command')
        t.see('No matching shortcuts.')
        t.keys('C-u')
        t.type('Space r p')
        compact = t.see('1 match')
        assert 'Go to page' in compact
        t.keys('Enter')
        t.see('[Esc Clear]')
        t.keys('Escape')
        guide=t.see('Find a key or action')
        t.click('Find a key or action',guide);t.type('related books')
        t.see('1 match');t.click('[Esc Clear]',t.see('[Esc Clear]'));t.see('Open settings')
        t.run('resize-window','-t','shelf','-x','115','-y','40')
        # Wait for the wide layout itself; tmux can reflow the old 42-column
        # frame before BOTUI receives SIGWINCH and recalculates its hit boxes.
        t.wait('Wide shortcut index restored', lambda f: '▸ General' in f or 'Sections' in f and 'PDF, DjVu & comics' in f)
        t.keys('Escape')
        home=t.wait('Wide source chooser',lambda f:'Inside this library' in f and '[F2 Settings]' in f)
        t.click('[F2 Settings]',home);t.see('Catppuccin Mocha');t.keys('Enter')
        t.see('Interface theme · preview');t.click('Gruvbox Dark',t.see('Gruvbox Dark'));t.click('[Esc Cancel]',t.see('[Esc Cancel]'));assert cfg()['interface_theme']=='catppuccin-mocha'
        t.keys('Enter');t.click('Nord',t.see('Nord'));t.keys('Enter')
        t.click('Background',t.see('Background'));t.see('Opaque')
        t.click('Borders & covers',t.see('Borders & covers'));t.click('Square',t.see('Square'))
        t.wait('Square borders applied',lambda f:'Solid terminal color' in f and cfg()['borders']=='square')
        t.keys('Tab', 'Home')
        for index, hint in enumerate((
            'Preview light and dark book paper',
            'Zen shows just the page',
            'Preferred width of each text column',
            'Space between facing text pages',
            'Reflow repairs words split',
            'Display embedded illustrations',
            'Choose one page, a spread',
            'Indent the first line',
            'On keeps book spacing in addresses',
            '0, 1 or 2 blank lines between paragraphs',
            'divider between the contents sidebar',
            'Show the reading frame, page and percentage',
            'Lines per wheel tick',
            'Delay before command hints appear',
            'Palette for PDF, DjVu',
            'Keep document colors, invert the original page',
            'Background around CBZ/CBR pages',
        )):
            if index:
                t.keys('Down')
            t.see(hint)
        t.click('Column gap',t.see('Column gap'));t.keys('C-u');t.type('10');t.keys('Enter');t.see('10 cells')
        t.click('First-line indent',t.see('First-line indent'));t.keys('C-u');t.type('3');t.keys('Enter');t.see('3 cells')
        assert cfg()['paragraph_indent']==3
        assert compact_row(t.see('Compact blocks'), True) and spacing_saved(True, None)
        t.click('Paragraph spacing', t.see('Paragraph spacing'))
        t.click('0 blank lines', t.see('0 blank lines'))
        t.wait('Zero paragraph gap with compact blocks enabled', lambda f: compact_row(f, True) and spacing_saved(True, 0))
        t.click('Compact blocks', t.see('Compact blocks'))
        t.wait('Compact blocks off without changing the gap', lambda f: compact_row(f, False) and spacing_saved(False, 0))
        t.click('Paragraph spacing', t.see('Paragraph spacing'))
        t.click('2 blank lines', t.see('2 blank lines'))
        t.wait('Gap changed independently of compact blocks', lambda f: compact_row(f, False) and spacing_saved(False, 2))
        t.click('Paragraph spacing', t.see('Paragraph spacing'))
        t.click('From book', t.see('From book'))
        t.wait('Book spacing without changing compact blocks', lambda f: compact_row(f, False) and spacing_saved(False, None))
        t.click('Paragraph spacing', t.see('Paragraph spacing'))
        t.click('2 blank lines', t.see('2 blank lines'))
        t.wait('Explicit gap restored while compact blocks remain off', lambda f: compact_row(f, False) and spacing_saved(False, 2))
        # Use the keyboard for the same toggle; it must retain the chosen gap.
        t.keys('Up', 'Enter')
        t.wait('Compact blocks on without changing the gap', lambda f: compact_row(f, True) and spacing_saved(True, 2))
        # Each following row still opens its own editor after inserting the toggle.
        for label, title in (
            ('Contents width', 'Contents width · 12–100 cells'),
            ('Mouse wheel step', 'Mouse wheel step · 1–12 lines'),
            ('Space menu delay', 'Space menu delay · milliseconds'),
            ('PDF reading theme', 'PDF reading theme'),
            ('PDF page colors', 'PDF page colors'),
            ('Comic background', 'Comic background'),
        ):
            t.click(label, t.see(label))
            t.wait(title, lambda f: title in f and 'First-line indent' not in f)
            t.keys('Escape')
            t.see('First-line indent')
        t.click('Zen frame & progress',t.see('Zen frame & progress'))
        t.wait('Zen progress enabled',lambda _:cfg()['zen_progress'])
        t.keys('Tab', 'End', 'Up')
        t.see('divider between the navigation sidebar')
        t.click('Reading statistics',t.see('Reading statistics'));t.see('Editions opened');t.keys('Escape')
        t.click('Download limit',t.see('4096 MiB'));t.keys('C-u');t.type('2048');t.keys('Enter');t.see('2048 MiB');assert cfg()['max_download_mib']==2048
        t.keys('Tab')
        t.click('Server', t.see('Password command'))
        t.see('BookOrbit server URL')
        t.type('invalid-address')
        t.see('invalid-address')
        t.keys('Enter')
        rejected = t.see('Invalid BookOrbit server address.')
        assert not cfg()['bookorbit']['server']
        t.click('Server', rejected)
        t.see('BookOrbit server URL')
        server = 'https://example.invalid'
        t.type(server)
        t.see(server)
        t.keys('Enter')
        account = t.wait('Saved server and returned to account settings',
                         lambda f: 'Password command' in f and server in f and cfg()['bookorbit']['server'] == server)
        t.click('Username', account)
        t.see('BookOrbit username')
        t.type('tester')
        t.see('tester')
        t.keys('Enter')
        account = t.wait('Saved username and returned to account settings',
                         lambda f: 'Password command' in f and 'tester' in f and cfg()['bookorbit']['username'] == 'tester')
        t.click('Password ', account)
        t.see('Replace saved password')
        t.type('masked-test')
        assert 'masked-test' not in t.see('•••••••••••')
        t.keys('Enter')
        t.see('Stored · replace or clear')
        t.keys('Escape');t.see('Your TUI book reader.');t.keys('q')
        data=cfg();assert (data['borders'],data['interface_theme'],data['column_gap'],data['opaque_background'])==('square','nord',10,True)
        assert data['bookorbit']['password']=='masked-test'
        assert spacing_saved(True, 2)
    finally:t.run('kill-server',required=False)
    t=s.Terminal(tmp,root/'target/debug/botui',args=[str(root/'examples/first-light.fb2')])
    try:
        t.see('Space z Zen');t.keys('Space','z')
        footer = re.compile(r'\d+ / \d+  ·  \d+%')
        t.wait('Zen footer in text reader', lambda f: footer.search(f.splitlines()[-1]))
        t.keys('Space','p')
        t.wait('Zen footer hidden', lambda f: not footer.search(f) and not cfg()['zen_progress'])
        t.keys('F2');t.see('Nord')
        t.keys('Tab')
        t.wait('Independent paragraph settings survive restart',
               lambda f: compact_row(f, True) and re.search(r'Paragraph spacing\s+2 blank lines', f) and spacing_saved(True, 2))
        t.keys('Escape');t.wait('Returned to reading', lambda f:'Interface theme' not in f and 'Почта на пристани' in f);t.keys('F1');t.see('Keyboard shortcuts');t.see('EPUB, FB2 & Markdown');t.keys('Escape','Space','q')
    finally:t.run('kill-server',required=False)
if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='botui-settings-') as d:check(Path(d))
    print('Passed themed settings, independent paragraph spacing and compact blocks, checked controls, masked password, mouse, palettes and restart.')
