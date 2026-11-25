#!/usr/bin/env python3
"""Clips whose motion was measured from the picture without their lens's rolling shutter.

Before the fix (GOPR1585), a built-in lens's readout went unused: the motion beside the clip (<stem>.afp-motion) was
measured, and plays, with readout 0 while its lens has one, and the edges of the picture wobble. Measured again
(Library: Source Settings, Motion data, Measure again), the readout is in.

Reads only: the first line (the JSON header) of each .afp-motion under the folders given. Writes nothing.

    tools/list_readout_left_out.py <folder>... [--json]
"""
import json
import os
import sys


def header(path):
    with open(path, "rb") as f:
        return json.loads(f.readline().decode("utf-8"))


def left_out(h):
    """The lens's readout (ms) when the motion was measured without it, else None"""
    if (h.get("readout_ms") or 0) != 0:
        return None
    r = (h.get("lens") or {}).get("frame_readout_time")
    return abs(r) if r else None


def main(args):
    as_json = "--json" in args
    folders = [a for a in args if a != "--json"]
    if not folders:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    found = []
    for root in folders:
        for d, dirs, files in os.walk(root):
            # The app's own folders and the NAS's recycle bin aren't clips' folders
            dirs[:] = [x for x in dirs if not x.startswith(".") and x != "#recycle"]
            for name in files:
                if not name.endswith(".afp-motion"):
                    continue
                p = os.path.join(d, name)
                try:
                    h = header(p)
                except (OSError, ValueError) as e:
                    print(f"{p}: can't be read: {e}", file=sys.stderr)
                    continue
                r = left_out(h)
                if r:
                    clip = os.path.join(d, (h.get("clip") or {}).get("name") or name[: -len(".afp-motion")])
                    found.append({"clip": clip, "lens": h.get("lens_name", ""), "readout_ms": r})
    if as_json:
        print(json.dumps(found, indent=1, ensure_ascii=False))
    else:
        for f in found:
            print(f"{f['clip']}\t{f['readout_ms']:.1f} ms\t{f['lens']}")
        print(f"{len(found)} clip(s) to measure again", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
