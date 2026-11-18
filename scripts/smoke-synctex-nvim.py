#!/usr/bin/env python3
"""Real Neovim RPC and BOTUI windows, isolated from the user's desktop and settings."""
import json
import os
from pathlib import Path
import runpy
import select
import shlex
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
support = runpy.run_path(str(ROOT / "scripts/smoke-shelf.py"))
BINARY = Path(os.environ.get("BOTUI_BINARY", ROOT / "target/debug/botui")).resolve()
NVIM = shutil.which("nvim")
assert NVIM, "The SyncTeX integration smoke test requires Neovim"


def wait(description, predicate, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(.04)
    raise AssertionError(description)


def vim_json(value):
    return "json_decode('" + json.dumps(value, ensure_ascii=False).replace("'", "''") + "')"


class Fixture:
    def __init__(self, root):
        self.root = root
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        self.source = root / "chapter.tex"
        self.literal = root / "кириллица % # | ' \" <CR> {line}.tex"
        for file in (self.source, self.literal):
            file.write_text("first source line\nsecond source line\nthird source line\n")
        self.pdf = root / "draft.pdf"
        self.pdf.write_bytes(support["pdf"]())
        self.pdf.with_suffix(".synctex").touch()
        self.inverse_source(self.source)
        helper = root / "synctex"
        helper.write_text('''#!/usr/bin/env python3
import os, sys
from pathlib import Path
root = Path(os.environ["HOME"])
if sys.argv[1] == "view":
    line, column, source = sys.argv[3].split(":", 2)
    assert Path(source).is_file()
    print(f"Page:{line}\\nh:20\\nv:45\\nW:100\\nH:20")
else:
    assert sys.argv[1] == "edit"
    page = sys.argv[3].split(":", 1)[0]
    source = (root / "inverse-source").read_text()
    print(f"Input:{source}\\nLine:{page}\\nColumn:1")
''')
        helper.chmod(0o755)
        # These are two different terminal prefixes. Each creates a real process
        # inside our private tmux server, keeping every writer under its teardown.
        wrapper = '''#!/usr/bin/env python3
import json, os, shlex, subprocess, sys, time
from pathlib import Path
root = Path(__file__).resolve().parent
editor = Path(__file__).name == "editor terminal"
kind = "editor" if editor else "reader"
assert sys.argv[1] == "--" + kind + "-prefix", sys.argv
args = sys.argv[2:]
fd = os.open(root / "launches.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
os.write(fd, (json.dumps({"kind": kind, "args": args}) + "\\n").encode())
os.close(fd)
gate = root / (kind + "-gate")
deadline = time.monotonic() + 8
while gate.exists():
    assert time.monotonic() < deadline, "test did not release startup gate"
    time.sleep(.02)
if editor:
    args[1:1] = ["--headless", "--clean", "-n", "-i", "NONE"]
env = json.loads((root / "environment.json").read_text())
tmux = json.loads((root / "tmux.json").read_text())
command = ["/usr/bin/env", *(k + "=" + v for k, v in env.items()), *args]
flags = ["-d"] if editor else []
result = subprocess.run(tmux + ["new-window", *flags, "-P", "-F", "#{pane_id}",
    "-t", "shelf", "-n", kind, shlex.join(command)], env=env,
    text=True, capture_output=True, check=True)
(root / (kind + "-pane")).write_text(result.stdout.strip())
'''
        self.prefixes = {}
        for kind in ("editor", "reader"):
            path = root / (kind + " terminal")
            path.write_text(wrapper)
            path.chmod(0o755)
            self.prefixes[kind] = [str(path), "--" + kind + "-prefix"]
        config = root / "config/botui/config.toml"
        config.parent.mkdir(parents=True)
        config.write_text('synctex_mode = "neovim"\n' +
                          "synctex_nvim_terminal = " + json.dumps(self.prefixes["editor"]) + "\n" +
                          "synctex_botui_terminal = " + json.dumps(self.prefixes["reader"]) + "\n")
        self.term = support["Terminal"](root, Path("/usr/bin/sleep"), args=["300"])
        self.env = dict(self.term.env, XDG_RUNTIME_DIR=str(runtime),
                        PATH=str(root) + os.pathsep + os.environ["PATH"], NO_COLOR="1")
        (root / "environment.json").write_text(json.dumps(self.env))
        (root / "tmux.json").write_text(json.dumps(self.term.base))

    def inverse_source(self, source):
        pending = self.root / "inverse-source.tmp"
        pending.write_text(str(source))
        pending.replace(self.root / "inverse-source")

    def launches(self, kind):
        path = self.root / "launches.jsonl"
        return [row for row in map(json.loads, path.read_text().splitlines()) if row["kind"] == kind] if path.exists() else []

    def command(self, *args):
        return shlex.join(["/usr/bin/env", *(k + "=" + v for k, v in self.env.items()), *map(str, args)])

    def start_reader(self):
        self.term.run("new-window", "-P", "-F", "#{pane_id}", "-t", "shelf", "-n", "reader",
                      self.command(BINARY, self.pdf))
        self.term.see("1 / 3")

    def start_editor(self):
        socket = self.root / "peer.sock"
        self.term.run("new-window", "-d", "-t", "shelf", "-n", "peer",
                      self.command(NVIM, "--headless", "--clean", "-n", "-i", "NONE",
                                   "--listen", socket, "--", self.source))
        wait("The original Neovim RPC socket becomes ready", lambda: socket.exists())
        return socket

    def rpc(self, socket, expression, check=True):
        result = subprocess.run([NVIM, "--server", str(socket), "--remote-expr", expression],
                                env=self.env, text=True, capture_output=True, timeout=8)
        if check:
            assert result.returncode == 0, result.stderr
        return result

    def state(self, socket):
        value = self.rpc(socket, "luaeval('vim.fn.json_encode({file=vim.api.nvim_buf_get_name(0),"
                         "line=vim.api.nvim_win_get_cursor(0)[1],column=vim.api.nvim_win_get_cursor(0)[2],"
                         "modified=vim.bo.modified,text=vim.api.nvim_get_current_line()})')")
        return json.loads(value.stdout)

    def at(self, socket, file, line):
        def correct():
            state = self.state(socket)
            return state["file"] == str(file) and state["line"] == line and state["column"] == 1
        try:
            wait(f"Neovim reaches literal source {file.name}, line {line}, column 2", correct)
        except AssertionError as error:
            frame = self.term.run("capture-pane", "-p", "-t", "shelf").stdout
            raise AssertionError(f"{error}\nEditor: {self.state(socket)}\n"
                                 f"Launches: {self.launches('editor')}\n{frame}") from error

    def forward(self, line, peer=None, parallel=False):
        env = dict(self.env)
        if peer:
            env["NVIM"] = str(peer)
        command = [BINARY, "--synctex-forward", f"{line}:1:{self.source}", self.pdf]
        if parallel:
            return subprocess.Popen(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=15)
        assert result.returncode == 0, result.stderr
        self.term.see(f"SyncTeX · page {line}")

    def close_reader(self):
        pid = int(self.term.run("display-message", "-p", "-t", "shelf", "#{pane_pid}").stdout)
        fd = os.pidfd_open(pid)
        try:
            self.term.keys("C-c")
            assert select.select([fd], [], [], 8)[0], "BOTUI did not exit after Ctrl+C"
        finally:
            os.close(fd)

    def close(self):
        self.term.run("kill-server", required=False)


def source_first(root):
    f = Fixture(root)
    f.pdf.write_bytes(runpy.run_path(str(ROOT / "scripts/smoke-pdf-interaction.py"))["pdf"]())
    try:
        peer = f.start_editor()
        # Start from actual Neovim. Its child process carries $NVIM back to the
        # reader, so F8 must return to this editor without using its launcher.
        argv = [str(BINARY), "--synctex-forward", f"2:1:{f.source}", str(f.pdf)]
        launched = f.rpc(peer, "luaeval('(function(a) local id=vim.fn.jobstart(a); "
                        "return vim.fn.jobwait({id},15000)[1] end)(_A)', " + vim_json(argv) + ")")
        assert launched.stdout.strip() == "0", launched.stdout
        f.term.see("SyncTeX · page 2")
        f.term.keys("F8")
        f.at(peer, f.source, 2)
        assert len(f.launches("reader")) == 1 and not f.launches("editor")
        for page in (3, 1):
            f.forward(page, peer)
            f.inverse_source(f.literal)
            f.term.keys("F8")
            f.at(peer, f.literal, page)
        assert len(f.launches("reader")) == 1 and not f.launches("editor")

        def select_title():
            frame = f.term.wait("Page raster ready for selection", lambda s: "@@@@" in s)
            points = [(x, y) for y, row in enumerate(frame.splitlines())
                      for x, char in enumerate(row) if char == "@"]
            left, right = min(x for x, _ in points), max(x for x, _ in points) + 1
            top, bottom = min(y for _, y in points), max(y for _, y in points) + 1
            x = left + int((right - left) * 45 / 360)
            y = top + int((bottom - top) * 50 / 500)
            seq = f"\033[<0;{x + 1};{y + 1}M\033[<0;{x + 1};{y + 1}m"
            f.term.run("send-keys", "-t", "shelf", "-H", *[f"{b:02x}" for b in seq.encode()])
            f.term.see("SELECT")

        # A forward jump after mouse selection must return to reading, retaining
        # the target for F8 rather than a stale cursor from the selected page.
        select_title()
        f.forward(2, peer)
        frame = f.term.see("Space z Zen")
        assert "No text at this position" not in frame and "SELECT" not in frame, frame
        f.term.keys("F8")
        f.at(peer, f.literal, 2)
        f.forward(1, peer)
        select_title()
        f.forward(1, peer)
        f.term.see("Space z Zen")

        # A modified buffer may refuse a switch. Preserve it and show the error,
        # never force the edit or create another window to hide the refusal.
        f.rpc(peer, '''luaeval('(function() vim.cmd("enew"); vim.o.hidden=false; vim.api.nvim_buf_set_lines(0,0,-1,false,{"unsaved marker"}); return true end)()')''')
        f.term.keys("F8")
        f.term.wait("Modified-buffer refusal is visible", lambda frame:
                    "could not open the source" in frame or "No write since" in frame or "E37" in frame)
        state = f.state(peer)
        assert state["modified"] and state["text"] == "unsaved marker" and not f.launches("editor")
        f.rpc(peer, '''luaeval('(function() vim.bo.modified=false; return true end)()')''')

        # Close the reader, then send simultaneous first requests. One launcher
        # must own startup; the other requests reuse that new window.
        f.close_reader()
        requests = [f.forward(2, peer, parallel=True) for _ in range(3)]
        try:
            for request in requests:
                _, stderr = request.communicate(timeout=18)
                assert request.returncode == 0, stderr
        finally:
            for request in requests:
                if request.poll() is None:
                    request.kill()
                    request.wait(timeout=5)
        f.term.see("SyncTeX · page 2")
        assert len(f.launches("reader")) == 2 and not f.launches("editor")
        f.term.keys("F8")
        f.at(peer, f.literal, 2)
    finally:
        f.close()


def pdf_first(root):
    f = Fixture(root)
    try:
        f.start_reader()
        gate = root / "editor-gate"
        gate.touch()
        f.inverse_source(f.literal)
        f.term.keys("F8")
        wait("First inverse request launches an editor", lambda: len(f.launches("editor")) == 1)
        # The editor has not created its socket yet. A repeated F8 must finish
        # that startup instead of starting a second Neovim in the meantime.
        f.term.keys("F8", "F8")
        time.sleep(.2)
        gate.unlink()
        args = f.launches("editor")[0]["args"]
        socket = Path(args[args.index("--listen") + 1])
        wait("New Neovim socket is ready", lambda: socket.exists())
        f.at(socket, f.literal, 1)
        for page in (2, 3):
            f.forward(page, socket)
            f.term.keys("F8")
            f.at(socket, f.literal, page)
        assert len(f.launches("editor")) == 1 and not f.launches("reader")

        # Closing the editor allows one replacement window; it does not leave
        # the reader tied to a dead RPC socket.
        f.rpc(socket, "execute('qa!')", check=False)
        wait("Closed Neovim removes its socket", lambda: not socket.exists())
        f.term.keys("F8")
        wait("Closed editor is reopened once", lambda: len(f.launches("editor")) == 2)
        wait("Replacement Neovim socket is ready", lambda: socket.exists())
        f.at(socket, f.literal, 3)
        assert not f.launches("reader")
    finally:
        f.close()


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-nvim-") as tmp:
        root = Path(tmp)
        for name, check in [("source-first", source_first), ("pdf-first", pdf_first)]:
            folder = root / name
            folder.mkdir()
            check(folder)
            print(f"PASS SyncTeX {name}: real RPC, isolated terminals and window reuse", flush=True)
