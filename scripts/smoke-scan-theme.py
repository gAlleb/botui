#!/usr/bin/env python3
"""Separate reading paper from interface colors across PDF modes and F2 reload."""
import importlib.util, os, tempfile
from pathlib import Path
spec=importlib.util.spec_from_file_location('sources',Path(__file__).with_name('smoke-sources.py'))
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
binary=Path(os.environ.get('BOTUI_BINARY',s.shelf.ROOT/'target/debug/botui')).resolve()
with tempfile.TemporaryDirectory(prefix='botui-scan-theme-') as tmp:
    root=Path(tmp); book=root/'paper.pdf';book.write_bytes(s.pdf(True))
    config=root/'config/botui/config.toml';config.parent.mkdir(parents=True)
    def settings(theme,interface):
        config.write_text(f'theme = "{theme}"\ninterface_theme = "{interface}"\nopaque_background = true\n')
    settings('sepia-dark','catppuccin-mocha')
    t=s.shelf.Terminal(root,binary,args=[str(book)])
    def ansi(): return t.run('capture-pane','-p','-e','-t','shelf').stdout
    try:
        t.see('1 / 3');t.keys('Space','t');t.see('Inverted colors')
        t.keys('Space','t');t.see('Reading theme')
        t.wait('Sepia reading paper',lambda _: '48;2;52;46;37' in ansi())
        t.keys('F2');t.see('Appearance')
        assert '48;2;30;30;46' in ansi(), 'F2 must keep its interface palette'
        settings('sepia-light','nord');t.keys('Escape')
        t.wait('Updated light paper without reopening PDF',lambda _: '48;2;241;232;208' in ansi())
        t.keys('Space','t');t.see('Original colors')
        t.keys('Space','t');t.see('Inverted colors')
        t.keys('Space','t');t.see('Reading theme')
        assert '48;2;241;232;208' in ansi()
        t.keys('q')
    finally: t.run('kill-server',required=False)
print('PASS PDF original/inverted/reading palette, independent F2 and live reading theme reload')
