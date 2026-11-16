#!/usr/bin/env python3
"""Exercise a non-interactive editor request, local-PDF routing and the inverse editor arguments."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

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
import os, sys
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
    print(f"Input:{os.environ['TEST_SOURCE']}\\nLine:{page}\\nColumn:-1")
""")
    helper.chmod(0o755)
    editor = root / "test editor"
    editor.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
(Path(os.environ["HOME"]) / "last-jump").write_text(json.dumps(sys.argv[1:]))
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

    try:
        term.see("1 / 3")
        jump(2)
        term.see("SyncTeX · page 2")
        term.keys("F8")
        term.see(f"{source}:2:1")
        term.wait("Editor argv contains the literal path", lambda _:
                  (root / "last-jump").exists() and
                  json.loads((root / "last-jump").read_text()) == ["2", "1", str(source)])
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
    print("SyncTeX: editor routing, literal arguments, concurrent PDF replacement and jump history passed")
