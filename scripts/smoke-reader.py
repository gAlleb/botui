#!/usr/bin/env python3
"""Exercise the built botui binary on the example FB2 using a private tmux terminal."""

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
import re

ROOT = Path(__file__).resolve().parents[1]
BOOK = ROOT / "examples/first-light.fb2"
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/botui"


def check(tmp):
    env = dict(os.environ, LC_ALL="C.UTF-8")
    env.pop("TMUX", None)
    env["XDG_DATA_HOME"]=str(tmp/"data")
    env["XDG_CONFIG_HOME"]=str(tmp/"config")
    (tmp/"config/botui").mkdir(parents=True)
    (tmp/"config/botui/reader-keys.json").write_text(json.dumps({"c":"contents"}))


    def tmux(*args, required=True):
        command = ["tmux", "-f", "/dev/null", "-S", str(tmp / "tmux.sock"), *args]
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        if required and result.returncode:
            raise RuntimeError(result.stderr.strip() or f"tmux {args[0]} failed")
        return result

    def open_book():
        launch = shlex.join(["env", f"XDG_STATE_HOME={tmp}", str(BINARY), str(BOOK)])
        tmux("new-session", "-d", "-s", "reader", "-x", "120", "-y", "22", launch)

    def wait_for(description, predicate):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            frame = tmux("capture-pane", "-p", "-t", "reader").stdout
            if predicate(frame):
                return frame
            time.sleep(0.04)
        raise AssertionError(f"{description}\nПоследний экран:\n{frame}")

    def keys(*values):
        tmux("send-keys", "-t", "reader", *values)

    def mouse(button, col, row):
        data = f"\033[<{button};{col};{row}M"
        if button == 0:
            data += f"\033[<0;{col};{row}m"
        keys("-H", *[f"{byte:02x}" for byte in data.encode()])

    def close_book():
        keys('Space', 'q')
        for _ in range(100):
            if tmux("has-session", "-t", "reader", required=False).returncode:
                return
            time.sleep(0.04)
        raise AssertionError("Книга не закрылась")

    def columns(frame):
        # On a 120-cell screen a single page is centered and at most 78 cells wide.
        return 2 if any(row[:20].strip() and row[62:].strip() for row in frame.splitlines()) else 1

    heading = "Почта на пристани"
    try:
        open_book()
        wait_for("Начальный разворот", lambda s: heading in s and columns(s) == 2)
        keys('Space', 'z');wait_for("Zen hides contents",lambda s:"Space z Zen" not in s and heading in s)
        # Showing a footer must not resize/reflow either page or move the passage.
        before = tmux("capture-pane", "-p", "-t", "reader").stdout.splitlines()
        keys('Space', 'p')
        shown = wait_for("Zen progress", lambda s: re.search(r"\d+ / \d+  ·  \d+%", s)).splitlines()
        assert shown[:-1] == before[:-1], "Progress footer moved the book text"
        keys('Space', 'p')
        hidden = wait_for("Hide zen progress", lambda s: not re.search(r"\d+ / \d+  ·  \d+%", s)).splitlines()
        assert hidden == before, "Hiding progress changed the page"
        keys('Space', 'm')
        actions = ("Resume", "Single page", "Two-page spread", "Theme", "Add bookmark", "Page width & column gap")
        menu = wait_for("Меню по M", lambda s: all(name in s for name in actions))
        def key_column(frame, action):
            line = next(line for line in frame.splitlines() if action in line)
            match = re.search(r"(?:Esc|Space [a-z?](?: [a-z0-9])?)(?=│)", line)
            assert match, line
            return match.end()
        assert len({key_column(menu, name) for name in actions}) == 1
        row,line = next((i,line) for i,line in enumerate(menu.splitlines(),1) if "Page width & column gap" in line)
        mouse(0,line.index("Page width")+1,row)
        panel = wait_for("Live page layout",lambda f:"Column gap" in f and "[ Done ]" in f)
        row,line = next((i,line) for i,line in enumerate(panel.splitlines(),1) if "Column gap" in line)
        mouse(0,line.index("[+]")+2,row)
        wait_for("Mouse widens gap",lambda f:"5 cells" in f)
        keys("-l","{")
        wait_for("Keyboard narrows gap",lambda f:"4 cells" in f)
        row,line = next((i,line) for i,line in enumerate(panel.splitlines(),1) if "Page width" in line)
        mouse(0,line.index("[-]")+2,row)
        config = tmp/"config/botui/config.toml"
        wait_for("Width saved separately",lambda f: "cells" in f and tomllib.loads(config.read_text()).get("page_width",0)>0)
        keys("Escape")
        wait_for("Close live layout",lambda f:"[ Done ]" not in f)
        keys('Space', 'm')
        menu=wait_for("Return to menu",lambda f:"Single page" in f)
        label = "Single page"
        row, line = next((i, line) for i, line in enumerate(menu.splitlines(), 1) if label in line)
        mouse(0, line.index(label) + 1, row)
        wait_for("Single page", lambda s: heading in s and label not in s and columns(s) == 1)
        keys('Space', 'l', '2')
        wait_for("Two-page spread с клавиатуры", lambda s: heading in s and columns(s) == 2)

        keys("c")
        toc=wait_for("Оглавление",lambda s:"Sections:" in s and "Обратный рейс" in s)
        for row, line in enumerate(toc.splitlines(), 1):
            entry = re.search(r"│[ ▸▾]*(Обратный рейс) *│", line)
            if entry:
                mouse(0, entry.start(1) + 1, row)
                break
        else:
            raise AssertionError(f"Chapter missing from the contents tree\n{toc}")
        wait_for("Переход к главе без панели",lambda s:s.strip().startswith("Обратный рейс") and "Sections:" not in s)
        keys('Space', 'b', 'a', 'Space', 'b', 'l')
        wait_for("Поставленная закладка",lambda s:"Bookmarks: 1" in s)
        keys("Escape");wait_for("Закрыли закладки",lambda s:"Bookmarks:" not in s);keys('Space', 'f');wait_for("Строка поиска",lambda s:"Find:" in s);keys("-l",heading);wait_for("Найден заголовок",lambda s:heading in s and " 1 " in s);keys("Enter")
        wait_for("Поиск вернул на начало главы",lambda s:heading in s and "Find:" not in s)
        keys('Space', 'a', 'n');wait_for("Comment editor",lambda s:"Ctrl+S Save" in s);keys("-l","A note at the pier");keys("C-s");keys('Space', 'a', 'a')
        wait_for("Saved note",lambda s:"A note at the pier" in s);keys("Enter");keys('Space', 'a', 'e')
        wait_for("Exported notes",lambda s:"Exported" in s)
        exported,=(tmp/"data/botui/exports").glob("*.md")
        assert "A note at the pier" in exported.read_text()
        keys("Escape");wait_for("Range cleared",lambda s:"[Esc Clear]" not in s)
        keys("Escape");wait_for("Selection closed",lambda s:"[Y Copy]" not in s)
        keys('Space', 't')
        wait_for("Палитры BookOrbit", lambda s: "Sepia" in s and "Terminal" in s)
        keys("Down", "Down", "Down", "Right", "Enter")
        wait_for("Применение схемы", lambda s: heading in s and "Terminal" not in s)

        mouse(65, 25, 8)  # SGR mouse wheel down
        page = wait_for("Перелистывание колесом", lambda s: heading not in s and bool(s.strip()))
        # Wait until the tmux capture contains the complete new frame.
        time.sleep(0.15)
        page=tmux("capture-pane","-p","-t","reader").stdout
        close_book()
        saved, = (tmp / "botui/positions").glob("*.json")
        record = json.loads(saved.read_text())
        assert record["spread"] and record["theme"] == "sepia-dark" and record["block"] > 0, record
        open_book()
        wait_for("Возврат на прочитанное место", lambda s: s == page)
        keys('Space', 'r', 's');wait_for("Statistics persist across sessions",lambda s:"Sessions on this device  2" in s)
        keys("Escape");wait_for("Return from statistics",lambda s:"Total time book open" not in s)
        close_book()
    finally:
        tmux("kill-server", required=False)


if __name__ == "__main__":
    if not shutil.which("tmux"):
        sys.exit("Для проверки нужен tmux.")
    if not BINARY.is_file():
        sys.exit("Сначала выполните cargo build --locked -p botui или передайте путь к бинарнику.")
    with tempfile.TemporaryDirectory(prefix="botui-smoke-") as temp:
        check(Path(temp))
    print("Пройдены меню, мышь, страница, разворот, выбор темы и восстановление позиции.")
