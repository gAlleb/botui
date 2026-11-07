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
        t.click('[? Help]',t.see('[? Help]'));t.see('Keyboard shortcuts');t.see('Open settings');t.keys('Escape')
        t.click('[F2 Settings]',t.see('[F2 Settings]'));t.click('Interface theme',t.see('Interface theme'))
        t.see('Interface theme · preview');t.click('Catppuccin Mocha',t.see('Catppuccin Mocha'));t.click('[Esc Cancel]',t.see('[Esc Cancel]'));assert cfg()['interface_theme']=='gruvbox-dark'
        t.keys('Enter');t.click('Nord',t.see('Nord'));t.keys('Enter')
        t.click('Background',t.see('Background'));t.see('Opaque')
        t.click('Borders & covers',t.see('Borders & covers'));t.click('Square',t.see('Square'))
        t.keys('Tab');t.click('Column gap',t.see('Column gap'));t.keys('C-u');t.type('10');t.keys('Enter');t.see('10 cells')
        t.click('First-line indent',t.see('First-line indent'));t.keys('C-u');t.type('3');t.keys('Enter');t.see('3 cells')
        assert cfg()['paragraph_indent']==3
        t.keys('Tab');t.click('Reading statistics',t.see('Reading statistics'));t.see('Editions opened');t.keys('Escape')
        t.keys('Tab');t.see('Password command');t.click('Server',t.see('Server'));t.type('https://example.invalid');t.keys('Enter')
        t.click('Username',t.see('Username'));t.type('tester');t.keys('Enter')
        t.click('Password ',t.see('Password '));t.type('masked-test');assert 'masked-test' not in t.see('Replace saved password');t.keys('Enter')
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
