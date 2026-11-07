#!/usr/bin/env python3
"""Migrate a real legacy profile and exercise both password sources through the sign-in screen."""
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import threading
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/botui"
spec = importlib.util.spec_from_file_location("shelf", ROOT / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)

def initialize(tmp):
    env = dict(os.environ, HOME=str(tmp), XDG_CONFIG_HOME=str(tmp/"config"), XDG_STATE_HOME=str(tmp/"state"))
    return subprocess.run([BIN, "--config-path"], env=env, capture_output=True, text=True)

def migration(tmp):
    cfg = tmp/"config/botui"
    cfg.mkdir(parents=True)
    (cfg/".config.lock").touch()
    originals = {"connection.json": {"server":"https://example.org", "username":"old", "list_view":True},
                 "config.json":{"library_root":"/example/books","theme":"sepia-dark","zen": True},
                 "reader-keys.json":{"ч":"contents"}}
    for name, content in originals.items():
        (cfg/name).write_text(json.dumps(content))
    procs = []
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=4) as pool:
        procs = list(pool.map(initialize,[tmp]*4))
    assert all(p.returncode == 0 for p in procs), [p.stderr for p in procs]
    path = cfg/"config.toml"
    data = tomllib.loads(path.read_text())
    assert data["keys"] == originals["reader-keys.json"] and data["bookorbit"] == originals["connection.json"]
    assert data["zen"] and data["theme"] == "sepia-dark" and data["library_root"] == "/example/books"
    assert set(cfg.iterdir()) == {path} and stat.S_IMODE(path.stat().st_mode) == 0o600
    for name, content in originals.items():
        backup, = (tmp/"state").rglob(name)
        assert json.loads(backup.read_text()) == content
    path.write_text('[bookorbit]\npassword = "SECRET_INVALID\n')
    error = initialize(tmp)
    assert error.returncode and "line 2" in error.stderr and "SECRET_INVALID" not in error.stderr
    # No partially written config and no deleted inputs if parsing the old config fails.
    path.unlink()
    (cfg/"config.json").write_text("{")
    assert initialize(tmp).returncode and (cfg/"config.json").read_text()=="{" and not path.exists()

def credentials(tmp):
    cfg = tmp/"config/botui"
    cfg.mkdir(parents=True)
    path = cfg/"config.toml"
    with shelf.Fixture(False) as server, shelf.Fixture(False) as other:
        for f in (server, other):
            threading.Thread(target=f.serve_forever,daemon=True).start()
        url = f"http://127.0.0.1:{server.server_port}/reader"
        alt = f"http://127.0.0.1:{other.server_port}/reader"
        marker = tmp/"command-used"
        helper = tmp/"helper with spaces"
        helper.write_text('#!/bin/sh\ntouch "$1"\nprintf "%s\\r\\nmetadata is not the password\\n" "$2"\n')
        helper.chmod(0o700)
        prefix = f'# Keep config comments\n[bookorbit]\nserver = "{url}"\nusername = "{shelf.USER}"\n'
        auth_cmd = "password_command = " + json.dumps([str(helper), str(marker), shelf.PASSWORD])
        for auth in [f'password = "{shelf.PASSWORD}"', auth_cmd, f'''password_command = "printf '%s' {shelf.PASSWORD}"''']:
            path.write_text(prefix + auth + ' # Keep credential comment\n')
            t = shelf.Terminal(tmp, BIN, url)
            try:
                t.see("Connect to BookOrbit")
                t.keys("Enter")
                t.see("Continue reading")
                t.keys("1")
                frame = t.see(shelf.BOOK)
                assert shelf.PASSWORD not in frame
                t.keys("v")
                t.see("[V Grid]")
                shelf.quit_shelf(t)
                assert "# Keep credential comment" in path.read_text() and "# Keep config comments" in path.read_text()
            finally:
                t.run("kill-server",required=False)
        assert marker.exists()
        marker.unlink()
        path.write_text(prefix + auth_cmd)
        t = shelf.Terminal(tmp,BIN,alt)
        try:
            shelf.login(t,alt)
            shelf.quit_shelf(t)
            assert not marker.exists() and tomllib.loads(path.read_text())["bookorbit"]["server"] == url
        finally:
            t.run("kill-server",required=False)
        # A failed helper may print a secret before exiting. It cannot authenticate or leak it.
        path.write_text(prefix + f'''password = "{shelf.PASSWORD}"\npassword_command = "echo SECRET_SENTINEL; exit 1"\n''')
        before = server.requests.count(("POST",shelf.API+"/auth/login",200))
        t = shelf.Terminal(tmp,BIN,url)
        try:
            t.see("Connect to BookOrbit")
            t.keys("Enter")
            frame=t.see("password command failed")
            assert "SECRET_SENTINEL" not in frame
            assert server.requests.count(("POST",shelf.API+"/auth/login",200)) == before
            t.type(shelf.PASSWORD)
            t.keys("Enter")
            t.see(shelf.BOOK)
            shelf.quit_shelf(t)
        finally:
            t.run("kill-server",required=False)
        for f in (server, other):
            f.shutdown()

if __name__=="__main__":
    with tempfile.TemporaryDirectory(prefix="botui-config-") as directory:
        root=Path(directory)
        migration(root/"migration")
        credentials(root/"auth")
    print("Passed TOML migration, concurrent startup, credential sourcing, account isolation and error redaction.")
