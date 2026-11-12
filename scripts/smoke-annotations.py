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
    for kind in os.environ.get("BOTUI_TEST_FORMATS", "fb2 epub md").split():
        tmp = root / kind
        tmp.mkdir()
        (tmp / "config/botui").mkdir(parents=True)
        (tmp / "config/botui/config.toml").write_text('zen = true\nlookup_command = '+json.dumps([str(bin_dir / "dictionary"), "--text", "{text}"])+"\n")
        path = tmp / f"study.{kind}"
        prose = "Alpha <em>bravo</em> charlie delta. Привет мир."
        paragraphs = "".join(f"<p>Paragraph {i:03} for navigation and stable reading.</p>" for i in range(90))
        if kind == "md":
            path.write_text("# Annotation chapter\n\n> Alpha *bravo* charlie delta. Привет мир.\n\n" + "\n\n".join(f"Paragraph {i:03} for navigation and stable reading." for i in range(90)))
        elif kind == "fb2":
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
            frame = t.see("Highlight")
            click(t, point(frame, "2 Green"))
            frame = t.see("Green ✓")
            click(t, point(frame, "[Enter Save]"))
            t.see("Annotation saved")
            assert notes(tmp)[0]["quote"] == "bravo", notes(tmp)
            assert notes(tmp)[0]["color"] == "green"
            assert "48;2;164;206;133" in t.run("capture-pane", "-p", "-e", "-t", "shelf").stdout
            saved_id = notes(tmp)[0]["id"]
            if kind == "md":
                saved_range = (notes(tmp)[0]["block"], notes(tmp)[0]["byte"], notes(tmp)[0]["end"])
                t.keys("F9")
                t.see("*bravo*")
                time.sleep(.5)
                click(t, point(t.see("*bravo*"), "bravo"))
                t.see("Ctrl+S Save")
                t.keys("Escape", "Escape", "Space", "r", "v")
                t.see("Alpha bravo")
                assert (notes(tmp)[0]["block"], notes(tmp)[0]["byte"], notes(tmp)[0]["end"]) == saved_range

            time.sleep(.5)
            click(t, point(t.see("bravo"), "bravo"))
            footer = t.see("Ctrl+S Save")
            assert "[Ctrl+S Save]  [Esc Cancel]  [Enter New line]  [Tab Color]" in footer
            for width in [52, 80, 115]:
                t.run("resize-window", "-t", "shelf", "-x", str(width), "-y", "30")
                right = (width - min(width, 82)) // 2 + min(width, 82)
                footer = t.wait("editor resized", lambda f: any(" Comment " in line and len(line.rstrip()) == right for line in f.splitlines()) and "[Tab Color]" in f)
                for label in ["[Ctrl+S Save]", "[Esc Cancel]", "[Enter New line]"]:
                    assert label in footer, footer
                click(t, point(footer, "3 Blue"))
                footer = t.see("Blue ✓")
                click(t, point(footer, "2 Green"))
                t.see("Green ✓")
            t.type("First observation")
            t.keys("Enter")
            t.type("  Second line  ")
            # Merging and splitting a line must preserve Markdown indentation.
            t.keys("Home", "BSpace", "Enter")
            t.keys("C-s")
            t.see("Annotation saved")
            assert len(notes(tmp)) == 1 and notes(tmp)[0]["id"] == saved_id
            assert notes(tmp)[0]["body"] == "First observation\n  Second line  ", notes(tmp)
            # Range addresses survive width, spread and contents pane changes.
            t.keys("Escape")
            t.wait("text cursor cleared", lambda s: "[Space a y Copy]" not in s)
            t.keys("Space", "l", "w", "]", "Enter", "Space", "l", "2", "Space", "z")
            t.see("Contents")
            t.keys("Space", "a", "a")
            frame = t.see("Highlights & notes · 1")
            assert "Book " in frame and "%" in frame, frame
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
            # keeps its highlight; choosing None as well removes the annotation.
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
            t.see("Highlights & notes · 1")
            assert len(notes(tmp)) == 1 and notes(tmp)[0]["color"] == "green"
            t.keys("n")
            frame = t.see("Ctrl+S Save")
            click(t, point(frame, "6 None"))
            frame = t.see("None ✓")
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
            # Right-click one word inside a long saved highlight: edit the same
            # annotation, preserving the full range rather than creating a duplicate.
            t.keys("Space", "a", "h", "Enter")
            t.see("Annotation saved")
            original = notes(tmp)[0].copy()
            t.keys("Space", "a", "a", "Enter")
            frame = t.see("bravo")
            t.keys("Escape")
            x, y = point(frame, "bravo")
            mouse(t, f"\033[<2;{x};{y}M\033[<2;{x};{y}m")
            t.see("Add note")
            t.keys("End", "Enter")
            t.see("Ctrl+S Save")
            t.type("Attached to the full highlight")
            t.keys("C-s")
            t.see("Annotation saved")
            saved = notes(tmp)
            assert len(saved) == 1 and saved[0]["id"] == original["id"], saved
            assert saved[0]["quote"] == original["quote"] and saved[0]["end"] == original["end"], saved
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
print("Passed text-format exact selection, clipboard, annotation edits, reflow, search, export and persistence.")
