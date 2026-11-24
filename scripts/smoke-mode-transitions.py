#!/usr/bin/env python3
"""Check reading anchors and ordinary page turns after changing text layouts.

Requires tmux and a built botui, like the other reader smoke scripts. Override
the executable with BOTUI_BINARY. All fixtures and reader state are temporary.
"""
import importlib.util
import os
from pathlib import Path
import re
import tempfile


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("shelf", ROOT / "scripts/smoke-shelf.py")
shelf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shelf)


def start(directory, body, page_width=50, pagination_mode=None):
    directory.mkdir()
    config = directory / "config/botui"
    config.mkdir(parents=True)
    (config / "config.toml").write_text(
        'reading_layout = "single"\n'
        'zen = true\nzen_progress = true\n'
        f'page_width = {page_width}\npreserve_printed_lines = true\n'
        + (f'pagination_mode = "{pagination_mode}"\n' if pagination_mode else "")
    )
    book = directory / "transitions.fb2"
    # The chapter boundary matters: it enables the block-aware page index.
    book.write_text(
        "<FictionBook><body><section><title><p>StartMarker</p></title>"
        + body
        + "</section></body></FictionBook>"
    )
    binary = Path(os.environ.get("BOTUI_BINARY", ROOT / "target/debug/botui")).resolve()
    terminal = shelf.Terminal(
        directory, Path("/usr/bin/env"), args=["NO_COLOR=1", str(binary), str(book)]
    )
    terminal.see("StartMarker")
    return terminal


def paragraph_body(count):
    # Eighteen short paragraphs fit two single pages or one complete spread.
    return "".join(f"<p>Passage {number:03}.</p>" for number in range(count))


def passages(frame, right=False):
    # Read a spread in column order. Its right page can start above the first
    # paragraph on the left when the left page begins with a chapter title.
    rows = frame.splitlines()
    column = [row[len(row) // 2:] if right else row[:len(row) // 2] for row in rows]
    return re.findall(r"Passage \d{3}", "\n".join(column))


def first_passage(frame):
    return passages(frame)[:1]


def scrolled_middle(terminal):
    terminal.keys("Space", "l", "3")
    terminal.see("Passage 000")
    terminal.keys(*(["Down"] * 9))
    return terminal.wait(
        "Scroll reaches a position between ordinary page boundaries",
        lambda frame: first_passage(frame) == ["Passage 004"],
    )


def mode_cycle(directory):
    terminal = start(directory, paragraph_body(100), pagination_mode="keep_word")
    try:
        frame = scrolled_middle(terminal)
        anchor = first_passage(frame)
        # The final Single -> Spread used to replace this restored source
        # anchor with the beginning of a previously calculated page pair.
        for mode, name in (("1", "Single"), ("3", "Scroll"), ("2", "Spread"),
                           ("1", "Single"), ("2", "Spread")):
            terminal.keys("Space", "l", mode)
            terminal.wait(
                f"{name} keeps {anchor[0]} at the top of the left text column",
                lambda frame, spread=(mode == "2"):
                    first_passage(frame) == anchor
                    and bool(passages(frame, right=True)) == spread,
            )
    finally:
        terminal.run("kill-server", required=False)


def reflow_cycle(directory):
    # Full width in Single/Scroll and half width in Spread force actual word
    # wrapping. Preserve the source word, not an obsolete rendered row number.
    body = "<p>" + " ".join(f"W{number:04}" for number in range(1000)) + "</p>"
    terminal = start(directory, body, page_width=0, pagination_mode="keep_word")

    def top_words(frame, spread=False):
        for row in frame.splitlines()[1:-1]:
            words = re.findall(r"W\d{4}", row[:len(row) // 2] if spread else row)
            if words:
                return words
        return []

    try:
        terminal.see("W0000")
        terminal.keys("Space", "l", "3")
        terminal.see("W0000")
        terminal.keys(*(["Down"] * 9))
        frame = terminal.wait(
            "Scroll reaches the middle of a wrapped paragraph",
            lambda frame: bool(top_words(frame)) and "W0000" not in frame,
        )
        anchor = top_words(frame)[0]
        wide_row = len(top_words(frame))
        for mode, name in (("1", "Single"), ("3", "Scroll"), ("2", "Spread"),
                           ("1", "Single"), ("2", "Spread")):
            terminal.keys("Space", "l", mode)
            terminal.wait(
                f"{name} keeps source word {anchor} in the first reflowed text row",
                lambda frame, spread=(mode == "2"):
                    anchor in top_words(frame, spread)
                    and (len(top_words(frame, spread)) < wide_row if spread
                         else len(top_words(frame)) == wide_row),
            )
    finally:
        terminal.run("kill-server", required=False)


def single_after_back(directory):
    terminal = start(directory, paragraph_body(18), pagination_mode="keep_word")
    try:
        terminal.see("1 / 2")
        terminal.keys("Down")
        regular_second = terminal.wait(
            "The short document has an ordinary second page",
            lambda frame: "2 / 2" in frame and "Passage 017" in frame,
        )
        expected = passages(regular_second)
        terminal.keys("Up")
        terminal.wait("Return to the document start",
                      lambda frame: first_passage(frame) == ["Passage 000"])
        anchor = first_passage(scrolled_middle(terminal))
        assert anchor != expected[:1], "The test anchor must not be an ordinary page start"
        terminal.keys("Space", "l", "1")
        terminal.wait("Scroll -> Single preserves the source position",
                      lambda frame: first_passage(frame) == anchor)
        terminal.keys("Up")
        terminal.wait("PageUp reaches the beginning",
                      lambda frame: first_passage(frame) == ["Passage 000"])
        terminal.keys("Down")
        terminal.wait(
            "PageDown uses the ordinary second page, not the old scroll anchor",
            lambda frame: passages(frame) == expected and "2 / 2" in frame,
        )
        terminal.keys("Up")
        terminal.wait("Ordinary reverse page turn returns to the beginning",
                      lambda frame: first_passage(frame) == ["Passage 000"])
        terminal.keys("Down")
        terminal.wait("The obsolete scroll anchor does not return on another turn",
                      lambda frame: passages(frame) == expected)
    finally:
        terminal.run("kill-server", required=False)


def spread_after_back(directory):
    terminal = start(directory, paragraph_body(18), pagination_mode="keep_word")
    try:
        anchor = first_passage(scrolled_middle(terminal))
        terminal.keys("Space", "l", "2")
        terminal.wait(
            "Scroll -> Spread preserves the source position",
            lambda frame: first_passage(frame) == anchor and bool(passages(frame, right=True)),
        )
        terminal.keys("Up")
        full = terminal.wait(
            "PageUp shows the whole short document in one spread",
            lambda frame: passages(frame) + passages(frame, right=True)
                == [f"Passage {number:03}" for number in range(18)],
        )
        terminal.keys("Down")
        terminal.wait(
            "The last spread does not advance back to the obsolete partial view",
            # A forward key can mark the book finished and update the frame's
            # progress labels; only the document content must stay in place.
            # The completed progress label also proves Down was handled, so
            # this cannot pass by capturing the old frame before the redraw.
            lambda frame: "100%" in frame.splitlines()[-1]
                and frame.splitlines()[1:-1] == full.splitlines()[1:-1],
        )
    finally:
        terminal.run("kill-server", required=False)


def chapter_pages(directory, mode):
    terminal = start(directory, paragraph_body(100))

    def visible(frame):
        return passages(frame) + (passages(frame, right=True) if mode == "2" else [])

    try:
        terminal.keys("Space", "l", mode)
        regular = terminal.wait("Ordinary chapter opening", lambda f:
                                bool(passages(f, right=True)) == (mode == "2") and "Passage 000" in f)
        opening = visible(regular)
        terminal.keys("Right")
        second = visible(terminal.wait("Next ordinary view", lambda f:
                                      visible(f) and visible(f)[0] not in opening))
        terminal.keys("Left")
        terminal.wait("Previous ordinary view", lambda f: visible(f) == opening)
        anchor = first_passage(scrolled_middle(terminal))
        terminal.keys("Space", "l", mode)
        terminal.wait("Return from Scroll to the whole chapter page", lambda f: visible(f) == opening)
        terminal.keys("Space", "l", "3")
        terminal.wait("Scroll restores the word originally selected", lambda f:
                      first_passage(f) == anchor and not passages(f, right=True))
        terminal.keys("Space", "l", mode)
        terminal.wait("Reopen the ordinary chapter page", lambda f: visible(f) == opening)
        terminal.keys("Right")
        terminal.wait("Forward from restored page", lambda f: visible(f) == second)
        terminal.keys("Left")
        terminal.wait("Back to the same page", lambda f: visible(f) == opening)
    finally:
        terminal.run("kill-server", required=False)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="botui-mode-transitions-") as tmp:
        directory = Path(tmp)
        mode_cycle(directory / "cycle")
        reflow_cycle(directory / "reflow")
        single_after_back(directory / "single")
        spread_after_back(directory / "spread")
        for mode in ("1", "2"):
            chapter_pages(directory / f"chapter-{mode}", mode)
    print("Passed layout anchors and normal page turns after leaving a scroll position.")
