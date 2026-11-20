#!/usr/bin/env python3
"""Reader mode round trips and direct Q back to a local shelf, in private tmux."""
import importlib.util
import os
import tempfile
import time
import tomllib
from pathlib import Path

spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
BINARY = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui')).resolve()


def check(home, kind):
    library = home / 'Library'
    folder = library / 'Nested'
    folder.mkdir(parents=True)
    book = folder / f'Navigation.{kind}'
    data = {'epub': s.epub, 'pdf': s.pdf,
            'fb2': lambda: (s.ROOT / 'examples/first-light.fb2').read_bytes()}[kind]()
    book.write_bytes(data)
    config = home / 'config/botui/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text('local_grid = false\nzen = true\nreading_layout = "single"\nleader_delay_ms = 1\n')
    term = s.Terminal(home, Path('/usr/bin/env'), args=['NO_COLOR=1', str(BINARY), '--local', str(library)])
    try:
        term.see('Nested/')
        term.keys('Enter'); term.see(book.name)
        term.keys('Enter')
        def reading():
            return term.wait('Book visible', lambda f: 'Local library' not in f and (
                'PDF' in f if kind == 'pdf' else s.EPUB_MARK in f if kind == 'epub' else s.FB2_MARK in f))
        if kind == 'pdf':
            term.keys('Space', 'z')
            term.see('1 / 3')
            term.keys('Right'); term.see('2 / 3')
        else:
            reading()
        # Every command group goes back to the root; closing root keeps the book.
        for group in ('r', 'b', 'a', 'd', 'l'):
            for back in ('q', 'Escape', 'BSpace'):
                term.keys('Space', group)
                term.see('Space · Q / Esc back')
                term.keys(back); term.see('Text cursor / Vim')
                term.keys('Escape')
                term.wait('Root closed', lambda f: 'Text cursor / Vim' not in f and 'Local library' not in f)
        # Visual -> Normal -> reading; toggling Space c also leaves Visual directly.
        term.keys('Space', 'c'); term.see('NORMAL')
        term.keys('v'); term.see('VISUAL')
        term.keys('Escape'); term.see('NORMAL')
        term.keys('Escape')
        term.wait('Esc leaves cursor mode', lambda f: 'NORMAL' not in f and 'VISUAL' not in f)
        term.keys('Space', 'c', 'v'); term.see('VISUAL')
        term.keys('Space', 'c')
        term.wait('Space c leaves cursor mode', lambda f: 'NORMAL' not in f and 'VISUAL' not in f)
        # Menu Q resumes reading; a second Q later closes the book.
        term.keys('Escape'); term.see('Resume')
        term.keys('q'); term.wait('Reading menu closed', lambda f: 'Resume' not in f and 'Local library' not in f)
        term.keys('F2'); term.see('Appearance')
        term.keys('Escape'); term.wait('Settings closed', lambda f: 'Appearance' not in f)
        term.keys('F1'); term.see('Keyboard shortcuts')
        term.keys('Escape'); term.wait('Help closed', lambda f: 'Keyboard shortcuts' not in f)
        term.keys('Space', 'l', 'w'); term.see('zoom' if kind == 'pdf' else 'Page layout')
        term.keys('q')
        term.wait('Width panel closed', lambda f: '[-]' not in f and '[ Done ]' not in f and 'Local library' not in f)
        if kind == 'pdf':
            for label in ('Invert original', 'Use reading theme', 'As in document'):
                term.keys('Space', 't')
                term.see('Page colors')
                term.keys(']')
                term.see(f'● {label}')
                term.keys('Enter')
                term.see(f'Page colors · {label}')
            term.keys('Space', 'r', 'p'); term.see('Go to page')
            term.keys('Escape'); term.wait('Page dialog closed', lambda f: '[ Go ↵ ]' not in f)
        else:
            term.keys('Space', 't'); term.see('Theme')
            term.keys('q'); term.wait('Palette closed', lambda f: 'Choose a theme' not in f and 'Local library' not in f)
        term.keys('Space', 'f'); term.see('Find')
        term.keys('q'); term.see('│ q ' if kind == 'pdf' else 'q▏')
        term.keys('Escape'); term.wait('Search closed', lambda f: 'Local library' not in f and 'Search PDF' not in f)
        term.keys('Space', 'a', 'a'); term.see('notes')
        term.keys('Escape')
        term.keys('1', '2', '3', '1')
        term.keys('q'); term.see('Local library'); term.see(book.name)
        # Reopen restores PDF page; uppercase Q works too.
        term.keys('Enter')
        if kind == 'pdf':
            term.see('2 / 3')
        else:
            reading()
        term.keys('Q'); term.see('Local library'); term.see(book.name)
        term.keys('q'); term.see('Nested/')
        term.keys('q'); term.see('Choose a library')
    finally:
        term.run('kill-server', required=False)


for kind in ('fb2', 'epub', 'pdf'):
    with tempfile.TemporaryDirectory(prefix=f'botui-navigation-{kind}-') as tmp:
        check(Path(tmp), kind)
    print(f'PASS {kind}: Space submenus, Normal/Visual, panels, direct q/Q, reopen and parent folders', flush=True)

def direct_file(home, extension):
    library = home / "Library"
    library.mkdir()
    (library / "saved library.md").write_text("Saved library")
    folder = home / "Downloads" / "Nested folder"
    folder.mkdir(parents=True)
    (folder / "aaa.md").write_text("First file in the list")
    book = folder / f"Книга с пробелами.{extension}"
    if extension == "pdf":
        book.write_bytes(s.pdf())
        marker = "1 / 3"
        argument = str(book)
    else:
        book.write_bytes((s.ROOT / "examples/first-light.fb2").read_bytes())
        marker = "Почта на пристани"
        argument = "./" + str(book.relative_to(home))
    config = home / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'library_root = "{library}"\nlocal_grid = false\n')
    term = s.Terminal(home, BINARY, args=[argument])
    try:
        term.see(marker)
        term.keys("Escape")
        term.see("Resume")
        term.keys("Escape")
        term.wait("Esc returns to the same reader", lambda f: marker in f and "Resume" not in f)
        term.keys("q")
        term.see(str(folder))
        term.keys("Enter")
        term.see(marker)  # The book, rather than aaa.md, was selected.
        term.keys("q")
        for directory in book.parents:
            term.wait(f"Directory {directory}", lambda f: any(
                str(directory) in line and "[V Grid]" in line and
                line.split("[V Grid]")[0].rstrip().endswith(str(directory))
                for line in f.splitlines()))
            term.keys("q")
        term.see("Choose a library")
        term.keys("l")
        term.see("saved library.md")
        assert tomllib.loads(config.read_text())["library_root"] == str(library)
    finally:
        term.run("kill-server", required=False)

    for key, button in (("m", "[M Main menu]"), ("C-q", "[Ctrl+Q Quit]")):
        term = s.Terminal(home, BINARY, args=[argument])
        try:
            term.see(marker)
            term.run("set-window-option", "remain-on-exit", "on")
            term.keys("q")
            term.see(str(folder))
            frame = term.see(button)
            if extension == "pdf":
                term.click(button, frame)
            else:
                term.keys(key)
            if key == "m":
                term.see("Choose a library")
            else:
                deadline = time.monotonic() + 5
                while True:
                    status = term.run("display-message", "-p", "#{pane_dead}:#{pane_dead_status}").stdout.strip()
                    if status == "1:0":
                        break
                    assert time.monotonic() < deadline, f"BOTUI failed to quit cleanly: {status}"
                    time.sleep(0.05)
            assert tomllib.loads(config.read_text())["library_root"] == str(library)
        finally:
            term.run("kill-server", required=False)


for kind in ("fb2", "pdf"):
    with tempfile.TemporaryDirectory(prefix="botui-direct-") as tmp:
        direct_file(Path(tmp), kind)
    print(f"PASS direct {kind}: selected file, parents to /, main menu and saved library", flush=True)
