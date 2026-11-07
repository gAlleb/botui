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

ROOT = Path(__file__).resolve().parents[1]
BOOK = ROOT / "examples/first-light.fb2"
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/botui"


def check(tmp):
    env = dict(os.environ, LC_ALL="C.UTF-8")
    env.pop("TMUX", None)
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
        keys("q")
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
        mouse(0, 30, 6)
        menu = wait_for("Меню по щелчку", lambda s: "Продолжить чтение" in s)
        label = "Одна страница"
        row, line = next((i, line) for i, line in enumerate(menu.splitlines(), 1) if label in line)
        mouse(0, line.index(label) + 1, row)
        wait_for("Одна страница", lambda s: heading in s and label not in s and columns(s) == 1)
        keys("]");keys("2")
        wait_for("Разворот с клавиатуры", lambda s: heading in s and columns(s) == 2)

        keys("c")
        toc=wait_for("Оглавление",lambda s:"Разделов:" in s and "Обратный рейс" in s)
        row,line=next((i,l) for i,l in enumerate(toc.splitlines(),1) if "Обратный рейс" in l and " 3 " in l)
        mouse(0,line.index("Обратный рейс")+1,row)
        wait_for("Переход к главе без панели",lambda s:s.strip().startswith("Обратный рейс") and "Разделов:" not in s)
        keys("b","g")
        wait_for("Поставленная закладка",lambda s:"Закладок: 1" in s)
        keys("Escape");wait_for("Закрыли закладки",lambda s:"Закладок:" not in s);keys("/");wait_for("Строка поиска",lambda s:"Найти:" in s);keys("-l",heading);wait_for("Найден заголовок",lambda s:heading in s and " 1 " in s);keys("Enter")
        wait_for("Поиск вернул на начало главы",lambda s:heading in s and "Найти:" not in s)
        keys("t")
        wait_for("Палитры BookOrbit", lambda s: "Сепия" in s and "Терминал" in s)
        keys("Down", "Down", "Down", "Right", "Enter")
        wait_for("Применение схемы", lambda s: heading in s and "Терминал" not in s)

        mouse(65, 25, 8)  # SGR mouse wheel down
        page = wait_for("Перелистывание колесом", lambda s: heading not in s and bool(s.strip()))
        close_book()
        saved, = (tmp / "botui/positions").glob("*.json")
        record = json.loads(saved.read_text())
        assert record["spread"] and record["theme"] == "sepia-dark" and record["margin"] == 1 and record["block"] > 0, record
        open_book()
        wait_for("Возврат на прочитанное место", lambda s: s == page)
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
