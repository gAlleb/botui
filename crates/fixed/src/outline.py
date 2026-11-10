import sys
from pathlib import Path
import gi
gi.require_version("Poppler", "0.18")
from gi.repository import Poppler

doc = Poppler.Document.new_from_file(Path(sys.argv[1]).as_uri(), None)

def walk(node, depth):
    while node is not None:
        action = node.get_action()
        if action and action.type == Poppler.ActionType.GOTO_DEST:
            dest = action.goto_dest.dest
            if dest and dest.type == Poppler.DestType.NAMED:
                dest = doc.find_dest(dest.named_dest)
            if dest and dest.page_num > 0:
                print(dest.page_num, depth, " ".join((action.any.title or "").split()), sep="\t")
        child = node.get_child() if depth < 32 else None
        if child:
            walk(child, depth + 1)
        if not node.next():
            break

walk(Poppler.IndexIter.new(doc), 0)
