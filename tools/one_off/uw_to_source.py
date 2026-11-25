#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""One-off: the underwater colour moved from a clip's transform to its source (Anton, 2026-11-24).

#273 kept uw, uw_highlights and uw_strength in the clip's transform bundle (its <clip>.afp.json's `view`). They're
the source's input now (its `source`, as the log curve), and the player and the Library read them only there: a
bundle's are left unread. This moves them, in each clip's own file under the folders given:
  - a key the `source` has already is kept (it's the newer), the bundle's dropped;
  - the bundle's other keys, and the file's, are kept as they are.
A moment's or clip preview's own bundle (in its link file) may hold them too: they're not read any more (a moment
plays its clip's), and this leaves them there.

NOT a feature: run once, by hand, on Anton's library, when he says so. It lists what it will do and writes nothing
without --go. Written 2026-11-24.

  python3 tools/one_off/uw_to_source.py <folder> [<folder> ...] [--go]
"""
import json, os, sys

KEYS = ("uw", "uw_highlights", "uw_strength")
SKIP_DIRS = {".afp", "_renders", "_cache", "#recycle", "@eaDir"}


def moved(doc):
    """The document with the bundle's underwater colour in its source: (changed, what moved, what was dropped)."""
    view = doc.get("view")
    if not isinstance(view, dict) or not any(k in view for k in KEYS):
        return False, {}, {}
    source = doc.get("source") if isinstance(doc.get("source"), dict) else {}
    took, dropped = {}, {}
    for k in KEYS:
        if k not in view:
            continue
        v = view.pop(k)
        if k in source:
            dropped[k] = v
        else:
            source[k] = v
            took[k] = v
    doc["source"] = source
    if not view:
        doc.pop("view")
    return True, took, dropped


def main(args):
    go = "--go" in args
    roots = [a for a in args if a != "--go"]
    if not roots:
        sys.exit(__doc__)
    n = 0
    for root in roots:
        for d, dirs, files in os.walk(root):
            dirs[:] = [x for x in dirs if x not in SKIP_DIRS and not x.startswith(".")]
            for f in files:
                if not f.endswith(".afp.json") or f.startswith("._"):
                    continue
                p = os.path.join(d, f)
                try:
                    with open(p, encoding="utf-8") as fh:
                        doc = json.load(fh)
                except (OSError, ValueError) as e:
                    print(f"skipped {p}: {e}")
                    continue
                changed, took, dropped = moved(doc)
                if not changed:
                    continue
                n += 1
                print(f"{p}: into source {json.dumps(took)}" + (f", dropped (source has its own) {json.dumps(dropped)}" if dropped else ""))
                if go:
                    tmp = p + ".tmp"
                    with open(tmp, "w", encoding="utf-8") as fh:
                        json.dump(doc, fh, ensure_ascii=False, indent=2)
                    os.replace(tmp, p)
    print(f"{n} file(s) {'changed' if go else 'to change (run with --go)'}")


if __name__ == "__main__":
    main(sys.argv[1:])
