#!/usr/bin/env python3
"""Exercise a non-interactive editor request, local-PDF routing and the inverse editor arguments."""
import json
import os
from pathlib import Path
import runpy
import shlex
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
support = runpy.run_path(str(ROOT / "scripts/smoke-shelf.py"))
BINARY = Path(os.environ.get("BOTUI_BINARY", ROOT / "target/debug/botui")).resolve()


def check(root):
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    source = root / "chapter (revised); {line}.tex"
    source.write_text("line one\nline two\n")
    pdf = root / "draft.pdf"
    pdf.write_bytes(support["pdf"]())
    (root / "draft.synctex").touch()
    # Control the model TeX output so CI can exercise the integration without installing a typesetter.
    helper = root / "synctex"
    helper.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
if sys.argv[1] == "view":
    assert sys.argv[2] == "-i" and sys.argv[4] == "-o"
    line, col, source = sys.argv[3].split(":", 2)
    assert Path(source).exists()
    print(f"Page:{line}\\nh:20\\nv:45\\nW:100\\nH:20")
else:
    assert sys.argv[1:3] == ["edit", "-o"]
    page, x, y, pdf = sys.argv[3].split(":", 3)
    assert Path(pdf).exists() and float(x) >= 0 and float(y) >= 0
    point = Path(os.environ["HOME"]) / "inverse-point"
    point.write_text(json.dumps([int(page), float(x), float(y)]))
    print(f"Input:{os.environ['TEST_SOURCE']}\\nLine:{page}\\nColumn:-1")
""")
    helper.chmod(0o755)
    editor = root / "test editor"
    editor.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
dest = Path(os.environ["HOME"]) / "last-jump"
pending = dest.with_suffix(".tmp")
pending.write_text(json.dumps(sys.argv[1:]))
pending.replace(dest)
""")
    editor.chmod(0o755)
    config = root / "config/botui/config.toml"
    config.parent.mkdir(parents=True)
    config.write_text("synctex_editor = " + json.dumps([str(editor), "{line}", "{column}", "{file}"]))
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), TEST_SOURCE=str(source),
               PATH=str(root) + os.pathsep + os.environ["PATH"])
    term = support["Terminal"](root, Path("/usr/bin/env"), args=[
        "XDG_RUNTIME_DIR=" + str(runtime), "PATH=" + env["PATH"], "TEST_SOURCE=" + str(source),
        "NO_COLOR=1", str(BINARY), str(pdf),
    ])

    def jump(line):
        result = subprocess.run([BINARY, "--synctex-forward", f"{line}:0:{source}", pdf],
                                env=env, capture_output=True, text=True, timeout=10)
        assert result.returncode == 0, result.stderr

    def editor_setting():
        term.keys("F2")
        term.see("Appearance")
        term.keys("Right", "End")
        return term.see("SyncTeX editor command")

    def mouse(x, y, button):
        seq = f"\033[<{button};{x + 1};{y + 1}M\033[<{button};{x + 1};{y + 1}m"
        term.run("send-keys", "-t", "shelf", "-H", *[f"{b:02x}" for b in seq.encode()])

    def source_reply():
        term.wait("Inverse request reached the configured editor", lambda _:
                  (root / "last-jump").exists())
        assert json.loads((root / "last-jump").read_text()) == ["1", "1", str(source)]
        return json.loads((root / "inverse-point").read_text())

    try:
        term.see("1 / 3")
        # Page one has graphics but no text. Source navigation must still be
        # available there and must use the original click, not the menu row.
        frame = term.wait("PDF raster for source context menu", lambda f: "@@@@" in f)
        points = [(x, y) for y, row in enumerate(frame.splitlines())
                  for x, char in enumerate(row) if char == "@"]
        x, y = points[len(points) // 2]
        mouse(x, y, 2)
        menu = term.see("Go to source")
        assert "│ Add note" not in menu and "│ Highlight" not in menu, menu
        term.click("Go to source", menu)
        first = source_reply()
        assert first[0] == 1
        (root / "last-jump").unlink()
        mouse(x, y, 2)
        term.see("Go to source")
        term.keys("End", "Enter")
        assert source_reply() == first
        (root / "last-jump").unlink()
        # Ctrl-left uses the same point without opening the context menu.
        mouse(x, y, 16)
        assert source_reply() == first
        assert "Go to source" not in term.run("capture-pane", "-p", "-t", "shelf").stdout

        jump(2)
        term.see("SyncTeX · page 2")
        term.keys("F8")
        term.see(f"{source}:2:1")
        term.wait("Editor argv contains the literal path", lambda _:
                  (root / "last-jump").exists() and
                  json.loads((root / "last-jump").read_text()) == ["2", "1", str(source)])
        # Edit the command in the open PDF, including a quoted executable path.
        term.click("SyncTeX editor command", editor_setting())
        field = term.see("[ Cancel ]")
        assert "{column}" in field and "{file}" in field, field
        args = [str(editor), "via F2", "{file}", "{line}:{column}"]
        term.keys("C-u")
        term.type(shlex.join(args))
        term.keys("Enter")
        term.see("SyncTeX editor command")
        assert tomllib.loads(config.read_text())["synctex_editor"] == args
        term.keys("Escape")
        term.see("[F1 Help]")
        term.keys("F8")
        term.wait("F8 uses the saved command without reopening the PDF", lambda _:
                  json.loads((root / "last-jump").read_text()) == ["via F2", str(source), "2:1"])

        editor_setting()
        term.keys("Enter")
        term.see("[ Cancel ]")
        term.keys("C-u", "Enter")
        term.see("Show source location")
        assert not tomllib.loads(config.read_text()).get("synctex_editor")
        (root / "last-jump").unlink()
        term.keys("Escape")
        jump(3)
        term.see("SyncTeX · page 3")
        term.keys("F8")
        term.see(f"{source}:3:1")
        assert not (root / "last-jump").exists()
        jump(2)
        term.see("SyncTeX · page 2")
        # Send the new page before the reader has had time to observe the rename.
        tmp = pdf.with_suffix(".tmp")
        tmp.write_bytes(support["pdf"](shades=(.2, .3, .4, .5)))
        tmp.replace(pdf)
        jump(4)
        term.see("4 / 4")
        term.keys("Space", "r", "[")
        term.see("2 / 4")
    finally:
        term.run("kill-server", required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-tex-") as d:
        check(Path(d))
    print("SyncTeX: editing and clearing the F8 command, literal arguments, PDF replacement and jump history passed")
