#!/usr/bin/env python3
"""Exercise real mouse/keyboard annotation flows in a private terminal and state dir."""
import importlib.util
import json
import os
import re
import sys
from pathlib import Path
import tempfile
import time
import zipfile

spec = importlib.util.spec_from_file_location("shelf", Path(__file__).with_name("smoke-shelf.py"))
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)
BINARY = Path(os.environ.get("BOTUI_BINARY", shelf.ROOT / "target/debug/botui")).resolve()


def mouse(t, seq):
    t.run("set-buffer", "-b", "mouse", seq)
    args = ["paste-buffer", "-d", "-r", "-b", "mouse", "-t", "shelf"]
    if t.run(*args, "-S", required=False).returncode:
        t.run(*args)


def point(frame, word):
    row, line = next((i, s) for i, s in enumerate(frame.splitlines(), 1) if word in s)
    return line.index(word) + 1, row


def click(t, xy, count=1):
    x, y = xy
    mouse(t, (f"\033[<0;{x};{y}M\033[<0;{x};{y}m") * count)


def notes(tmp):
    return json.loads(next((tmp / "state/botui/positions").glob("*.notes")).read_text())


with tempfile.TemporaryDirectory() as name:
    root = Path(name)
    bin_dir = root / "bin"
    bin_dir.mkdir()
    for helper in ["xclip", "wl-copy", "xsel", "pbcopy"]:
        (bin_dir / helper).write_text('#!/bin/sh\ncat > "$HOME/copied"\n')
        (bin_dir / helper).chmod(0o755)
    (bin_dir / "dictionary").write_text('#!/usr/bin/env python3\nimport json,os,sys\nfrom pathlib import Path\n(Path(os.environ["HOME"]) / "lookup-args").write_text(json.dumps(sys.argv[1:]))\nprint("Definition: " + sys.argv[-1])\n')
    (bin_dir / "dictionary").chmod(0o755)
    os.environ["PATH"] = str(bin_dir) + os.pathsep + os.environ["PATH"]
    for kind in ["fb2", "epub"]:
        tmp = root / kind
        tmp.mkdir()
        (tmp / "config/botui").mkdir(parents=True)
        (tmp / "config/botui/config.toml").write_text('zen = true\nlookup_command = '+json.dumps([str(bin_dir / "dictionary"), "--text", "{text}"])+"\n")
        path = tmp / f"study.{kind}"
        prose = "Alpha <em>bravo</em> charlie delta. Привет мир."
        paragraphs = "".join(f"<p>Paragraph {i:03} for navigation and stable reading.</p>" for i in range(90))
        if kind == "fb2":
            path.write_text('<FictionBook><body><section><p>' + prose.replace("em>", "emphasis>") + '</p>' + paragraphs + '</section></body></FictionBook>')
        else:
            with zipfile.ZipFile(path, "w") as z:
                z.writestr("mimetype", "application/epub+zip")
                z.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
                z.writestr("book.opf", '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Study</dc:title><dc:identifier id="id">study</dc:identifier></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>')
                z.writestr("c.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><head><style>p { margin: 1em 0; } em { font-style: italic; }</style></head><body><p>' + prose + '</p>' + paragraphs + '</body></html>')
        t = shelf.Terminal(tmp, BINARY, args=[str(path)])
        try:
            t.see("Alpha")
            t.keys("Space", "l", "1")
            frame = t.see("Alpha")
            xy = point(frame, "bravo")
            click(t, xy)
            assert "Reading" not in t.see("TEXT")
            t.keys("Escape")
            time.sleep(.5)
            click(t, xy, 2)
            t.see("SELECT")
            t.keys("Space", "a", "y")
            t.see("Copied to clipboard")
            assert (tmp / "copied").read_text() == "bravo"
            t.keys("Space", "d", "d")
            t.see("Definition: bravo")
            assert json.loads((tmp / "lookup-args").read_text()) == ["--text", "bravo"]
            t.keys("Escape")
            t.see("SELECT")
            t.keys("Space", "a", "h")
            t.see("Highlight")
            t.keys("2", "Enter")
            t.see("Annotation saved")
            assert notes(tmp)[0]["quote"] == "bravo", notes(tmp)
            assert notes(tmp)[0]["color"] == "green"
            assert "48;2;164;206;133" in t.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
            t.keys("Space", "a", "n")
            footer = t.see("Ctrl+S Save")
            assert "[Ctrl+S Save]  [Esc Cancel]  [Enter New line]  [Tab Color]" in footer
            for width in [52, 80, 115]:
                t.run("resize-window", "-t", "shelf", "-x", str(width), "-y", "30")
                footer = t.see("[Tab Color]")
                for label in ["[Ctrl+S Save]", "[Esc Cancel]", "[Enter New line]"]:
                    assert label in footer, footer
            t.type("First observation")
            t.keys("Enter")
            t.type("  Second line  ")
            # Merging and splitting a line must preserve Markdown indentation.
            t.keys("Home", "BSpace", "Enter")
            t.keys("C-s")
            t.see("Annotation saved")
            assert len(notes(tmp)) == 1
            assert notes(tmp)[0]["body"] == "First observation\n  Second line  ", notes(tmp)
            # Range addresses survive width, spread and contents pane changes.
            t.keys("Escape")
            t.wait("text cursor cleared", lambda s: "[Space a y Copy]" not in s)
            t.keys("Space", "l", "w", "]", "Enter", "Space", "l", "2", "Space", "z")
            t.see("Contents")
            t.keys("Space", "a", "a")
            t.see("Highlights & notes · 1")
            t.keys("/")
            t.type("unfindable")
            t.see("Highlights & notes · 0")
            t.keys("C-u")
            t.type("observation")
            t.see("Highlights & notes · 1")
            t.keys("Enter", "Enter")
            t.see("SELECT")
            t.keys("Space", "a", "y")
            t.see("Copied to clipboard")
            assert (tmp / "copied").read_text() == "bravo"
            t.keys("Space", "a", "e")
            t.see("Exported")
            exported = next((tmp / ".local/share/botui/exports").glob("*.md")).read_text()
            assert "> bravo" in exported and "Second line" in exported, exported
            t.keys("Escape", "Escape", "Space", "q")
            time.sleep(.2)
            t.run("kill-server", required=False)
            t = shelf.Terminal(tmp, BINARY, args=[str(path)])
            t.see("bravo")
            t.keys("Space", "a", "a")
            t.see("Highlights & notes · 1")
            # Cancel leaves the saved note intact. Saving a cleared comment
            # removes the whole annotation, including its colored range.
            t.keys("n")
            t.see("Ctrl+S Save")
            t.keys("C-u", "BSpace", "C-u", "Escape")
            t.see("Highlights & notes · 1")
            assert notes(tmp)[0]["body"] == "First observation\n  Second line  "
            t.keys("n")
            t.see("Ctrl+S Save")
            t.keys("C-u", "BSpace", "C-u")
            t.type("   ")
            frame = t.see("[Ctrl+S Save]")
            click(t, point(frame, "[Ctrl+S Save]"))
            t.see("Highlights & notes · 0")
            assert notes(tmp) == []
            # A deliberate color-only highlight remains valid with no comment.
            t.keys("Escape")
            t.wait("annotation list closed", lambda s: "Highlights & notes" not in s)
            t.keys("Space", "a", "h")
            t.see(" Highlight ")
            t.keys("Enter")
            t.see("Annotation saved")
            assert len(notes(tmp)) == 1 and notes(tmp)[0]["body"] == ""
            t.keys("Space", "a", "a")
            t.see("Highlights & notes · 1")
            t.keys("DC")
            t.see("Delete this annotation?")
            t.keys("Down", "Enter")
            t.see("Highlights & notes · 0")
            assert notes(tmp) == []
            t.keys("Escape")
            t.wait("annotation list closed", lambda s: "Highlights & notes" not in s)
            t.keys("Space", "c", "Home", "v", "w", "y")
            t.see("Copied to clipboard")
            assert (tmp / "copied").read_text() == "Alpha b", (tmp / "copied").read_text()
            # Re-enter Visual to exercise the complete clear button.
            t.keys("v")
            frame = t.see("[Esc Clear]")
            click(t, point(frame, "[Esc Clear]"))
            t.see("NORMAL")
            t.keys("m", "a", "w", "'", "a")
            t.keys("Space", "m")
            t.see("Resume")
            t.keys("Escape")
            t.see("bravo")
            t.keys("Space", "c")
            t.wait("normal mode cleared", lambda s: "[Space a y Copy]" not in s)
            frame = t.see("bravo")
            x, y = point(frame, "bravo")
            # Reverse mouse drag includes both endpoints, not just the last word.
            mouse(t, f"\033[<0;{x + 4};{y}M\033[<32;{x};{y}M\033[<0;{x};{y}m")
            t.see("SELECT")
            t.keys("Space", "a", "y")
            t.see("Copied to clipboard")
            assert (tmp / "copied").read_text() == "bravo"
            t.keys("Escape")
            t.see("TEXT")
            time.sleep(.5)
            click(t, (x, y), 3)
            t.see("SELECT")
            t.keys("Space", "a", "y")
            t.see("Copied to clipboard")
            assert (tmp / "copied").read_text() == "Alpha bravo charlie delta. Привет мир."
            t.keys("Escape")
            t.see("TEXT")
            t.keys("Escape")
            t.wait("Cursor cleared", lambda s: "[Space a y Copy]" not in s)
            t.keys("Space", "l", "3")
            frame=t.see("bravo")
            x,y=point(frame,"bravo")
            # Hold a drag below the text: extend beyond the viewport.
            mouse(t, f"\033[<0;{x};{y}M\033[<32;{x};30M")
            time.sleep(.65)
            mouse(t, f"\033[<0;{x};30m")
            t.keys("Space", "a", "y")
            t.see("Copied to clipboard")
            copy=(tmp / "copied").read_text()
            assert copy.startswith("bravo") and "Paragraph 006" in copy, copy
            before=t.run("capture-pane","-p","-t","shelf").stdout
            time.sleep(.25)
            assert before==t.run("capture-pane","-p","-t","shelf").stdout
        finally:
            t.run("kill-server", required=False)
    for i, file in enumerate(sys.argv[1:]):
        tmp=root/f"real-{i}";tmp.mkdir()
        (tmp/"config/botui").mkdir(parents=True)
        (tmp/"config/botui/config.toml").write_text('zen=true\nreading_layout="single"\n')
        t=shelf.Terminal(tmp,BINARY,args=[str(Path(file).resolve())])
        try:
            # There is no attached emulator to answer the first graphics probe
            # in this private tmux. Let its bounded 2s query complete before
            # sending input, then capture the final geometry of the page.
            time.sleep(2.2)
            frame=t.wait("real book text and illustrations",lambda f: len(re.findall(r"[A-Za-zА-Яа-я]{6,}",f))>3 and "Loading" not in f and "Illustration ·" not in f)
            word=re.search(r"[A-Za-zА-Яа-я]{6,}",frame).group()
            print(f"Selecting {word!r} in {Path(file).name}",flush=True)
            click(t,point(frame,word),2);t.see("SELECT");t.keys("Space", "a", "y");t.see("Copied to clipboard")
            assert (tmp/"copied").read_text()==word,(file,word,(tmp/"copied").read_text())
            t.keys("Space","l","w","]","Enter","Space","z","Space","a","y");t.see("Copied to clipboard")
            assert (tmp/"copied").read_text()==word
            print(f"Real book selection and reflow: {Path(file).name}",flush=True)
        finally:t.run("kill-server",required=False)
print("Passed FB2/EPUB exact selection, clipboard, annotation edits, reflow, search, export and persistence.")
