#!/usr/bin/env python3
"""PDF theme popup: real preview, cancel, apply, original/inverted and reopen."""
import importlib.util, os, tempfile, tomllib
from pathlib import Path
spec=importlib.util.spec_from_file_location('sources',Path(__file__).with_name('smoke-sources.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
binary=Path(os.environ.get('BOTUI_BINARY',s.shelf.ROOT/'target/debug/botui')).resolve()
with tempfile.TemporaryDirectory(prefix='botui-scan-theme-') as tmp:
    root=Path(tmp); book=root/'paper.pdf';book.write_bytes(s.pdf(True))
    config=root/'config/botui/config.toml';config.parent.mkdir(parents=True)
    config.write_text('theme = "sepia-dark"\ninterface_theme = "catppuccin-mocha"\nopaque_background = true\n')
    t=s.shelf.Terminal(root,binary,args=[str(book)])
    def ansi(): return t.run('capture-pane','-p','-e','-t','shelf').stdout
    def open_picker():
        t.keys('Space','t');f=t.see('Page theme')
        assert 'Original' in f and 'Inverted' in f and 'Sepia' in f
        assert '1 / 3' in f, 'Theme dialog hid the reader'
    try:
        t.see('1 / 3');open_picker()
        t.keys('Home','Down','Down','Down','Down','Down','Left')
        t.wait('Light paper live preview',lambda _: '48;2;241;232;208' in ansi())
        assert tomllib.loads(config.read_text())['scan_theme']=='original', 'Preview was persisted'
        t.keys('Escape');t.wait('Cancelled theme picker',lambda f:'Page theme' not in f)
        assert tomllib.loads(config.read_text())['theme']=='sepia-dark'
        open_picker();t.keys('Home','Down','Down','Down','Down','Down','Left','Enter')
        t.see('Reading theme');assert tomllib.loads(config.read_text())['scan_theme']=='sepia-light'
        assert tomllib.loads(config.read_text())['theme']=='sepia-dark', 'PDF changed EPUB theme'
        t.keys('F2');t.see('Appearance');assert '48;2;30;30;46' in ansi()
        t.keys('Right');t.see('Reading theme');t.keys('End');t.see('PDF / image theme')
        t.keys('Enter');t.see('PDF / image theme');t.keys('Home','Down','Enter')
        t.wait('F2 choice saved',lambda _:tomllib.loads(config.read_text())['scan_theme']=='inverted')
        t.keys('Escape');t.wait('Reader restored',lambda f:'Appearance' not in f)
        open_picker();t.click('Inverted',t.see('Page theme'));t.see('Inverted colors')
        open_picker();t.click('Original',t.see('Page theme'));t.see('Original colors')
        open_picker();t.keys('Home','Down','Down','Down','Down','Down','Right','Enter');t.see('Reading theme')
        assert tomllib.loads(config.read_text())['scan_theme']=='sepia-dark'
        assert tomllib.loads(config.read_text())['theme']=='sepia-dark'
        t.keys('q')
    finally:t.run('kill-server',required=False)
    other=root/'new-paper.pdf';other.write_bytes(book.read_bytes()+b'\n')
    t=s.shelf.Terminal(root,binary,args=[str(other)])
    try:
        t.see('1 / 3');t.wait('Saved dark paper',lambda _: '48;2;52;46;37' in ansi())
        open_picker();t.keys('Home','Down','Escape');t.wait('Cancel restores saved theme',lambda f:'Page theme' not in f)
        assert '48;2;52;46;37' in ansi()
        t.run('resize-window','-t','shelf','-x','54','-y','16')
        t.wait('Resized reader',lambda f:len(f.splitlines())<=16 and '1 / 3' in f)
        open_picker();t.keys('End');t.see('AMOLED');t.keys('Home');t.see('Original')
        t.click('Inverted',t.see('Original'));t.see('Inverted colors')
        open_picker();t.keys('End','Up','Right');t.see('Slate');t.keys('Enter');t.see('Reading theme')
        assert tomllib.loads(config.read_text())['scan_theme']=='slate-dark'
        t.keys('F2');t.click('Interface theme',t.see('Interface theme'))
        t.see('Interface theme · preview');t.keys('End');t.see('Plum');t.keys('Enter')
        t.wait('New interface palette saved',lambda _:tomllib.loads(config.read_text())['interface_theme']=='plum')
        t.keys('Escape');t.wait('PDF retains independent palette',lambda _: '48;2;28;37;48' in ansi())
        t.keys('q')
    finally:t.run('kill-server',required=False)
print('PASS Space PDF theme picker: preview without save, cancel, light/dark, mouse, Original/Inverted and reopen')
