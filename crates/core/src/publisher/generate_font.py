"""Rebuild the embedded cell metrics with FontTools. The terminal draws the glyphs."""

from pathlib import Path
from unicodedata import category, east_asian_width

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable


def glyph(char):
    if category(char) in ("Mn", "Me", "Cf"):
        return "mark"
    return "wide" if east_asian_width(char) in ("F", "W") else "cell"


def build(path):
    font = FontBuilder(1000, isTTF=True)
    names = [".notdef", "cell", "wide", "mark"]
    font.setupGlyphOrder(names)
    font.setupCharacterMap({i: "cell" for i in range(32, 127)})
    # Format 13 groups entire Unicode ranges onto just three metric glyphs.
    cmap = CmapSubtable.newSubtable(13)
    cmap.platformID, cmap.platEncID, cmap.language = 3, 10, 0
    cmap.cmap = {
        i: glyph(chr(i))
        for i in range(32, 0x110000)
        if category(chr(i)) not in ("Cc", "Cs", "Cn")
    }
    font.font["cmap"].tables.append(cmap)
    font.setupGlyf({name: TTGlyphPen(None).glyph() for name in names})
    font.setupHorizontalMetrics(dict(zip(names, [(500, 0), (500, 0), (1000, 0), (0, 0)])))
    font.setupHorizontalHeader(ascent=800, descent=-200)
    font.setupNameTable({
        "familyName": "BOTUI Cells",
        "styleName": "Regular",
        "uniqueFontIdentifier": "BOTUI Cells 1",
        "fullName": "BOTUI Cells",
        "psName": "BOTUI-Cells",
    })
    font.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    font.setupPost()
    font.setupMaxp()
    font.font["head"].created = font.font["head"].modified = 2082844800
    font.font.recalcTimestamp = False
    font.save(path)


if __name__ == "__main__":
    build(Path(__file__).with_name("cells.ttf"))
