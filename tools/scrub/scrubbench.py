#!/usr/bin/env python3
"""scrubbench: afp-scrub's timings as the seek bar would ask (docs/seek-and-scrub.md).

  tools/scrub/scrubbench.py [--helper path] [--cold] [--n N] [--width W --height H] <clip> [<sidecar>]

Runs afp-scrub (stdin/stdout), then for the sidecar (exact frames) and the clip (keyframes): N times spread over the
clip in a shuffled order (a mouse hovering here and there), the mouse moving along the bar (0.1 s steps, 30 of them),
the same shuffled times again (kept in memory), and two requests at once (the first dropped). Prints per scenario the
median / p90 / max of the time the helper took and of the round trip (request written -> reply read), and which kind
of answer it was. --cold: the files dropped from this Mac's cache first (msync MS_INVALIDATE, as thumbbench.c).
"""
import argparse
import ctypes
import mmap
import os
import random
import statistics
import subprocess
import sys
import tempfile
import time


def evict(path):
    """The file's pages dropped from the cache: the next read goes to the disk (or the NAS)"""
    libc = ctypes.CDLL(None, use_errno=True)
    libc.mmap.restype = ctypes.c_void_p
    libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
    libc.msync.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
    libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    size = os.path.getsize(path)
    fd = os.open(path, os.O_RDONLY)
    try:
        p = libc.mmap(None, size, mmap.PROT_READ, mmap.MAP_SHARED, fd, 0)
        if p not in (None, ctypes.c_void_p(-1).value):
            libc.msync(p, size, 2)  # MS_INVALIDATE on macOS
            libc.munmap(p, size)
    finally:
        os.close(fd)


class Helper:
    def __init__(self, exe):
        self.p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.n = 0
        self.dir = tempfile.mkdtemp(prefix="scrubbench-")

    def ask(self, file, t, mode, w, h, clip=None, wait=True):
        self.n += 1
        out = os.path.join(self.dir, f"{self.n}.bgra")
        fields = [str(self.n), file, f"{t:.3f}", mode, out, str(w), str(h)] + ([clip] if clip else [])
        a = time.monotonic()
        self.p.stdin.write("\t".join(fields) + "\n")
        self.p.stdin.flush()
        return (self.n, a, out) if not wait else self.read(self.n, a)

    def read(self, want, a):
        while True:
            parts = self.p.stdout.readline().rstrip("\n").split("\t")
            if parts[0] == str(want):
                rt = (time.monotonic() - a) * 1000
                if parts[1] == "ok":
                    return {"ok": True, "ms": float(parts[2]), "kind": parts[3], "t": float(parts[4]), "rt": rt}
                return {"ok": False, "status": parts[1], "rt": rt, "msg": parts[2:]}

    def close(self):
        self.p.stdin.close()
        self.p.wait()


def stats(rs):
    ok = [r for r in rs if r["ok"]]
    if not ok:
        return "no pictures: " + str(rs[:2])
    ms = sorted(r["ms"] for r in ok)
    rt = sorted(r["rt"] for r in ok)
    kinds = {}
    for r in ok:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    q = lambda v, f: v[min(len(v) - 1, int(len(v) * f))]
    return (f"helper median {statistics.median(ms):5.1f} p90 {q(ms, .9):5.1f} max {ms[-1]:5.1f} ms | round trip median "
            f"{statistics.median(rt):5.1f} p90 {q(rt, .9):5.1f} ms | {len(ok)}/{len(rs)} {kinds}")


def duration(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                         capture_output=True, text=True).stdout.strip()
    return float(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--helper", default=os.path.join(os.path.dirname(__file__), "afp-scrub"))
    ap.add_argument("--cold", action="store_true")
    ap.add_argument("--n", type=int, default=40)
    ap.add_argument("--width", type=int, default=462)
    ap.add_argument("--height", type=int, default=260)
    ap.add_argument("clip")
    ap.add_argument("sidecar", nargs="?")
    a = ap.parse_args()
    d = duration(a.clip)
    random.seed(7)
    times = [d * (i + 0.5) / a.n for i in range(a.n)]
    random.shuffle(times)
    walk = [d * 0.25 + i * 0.1 for i in range(30)]
    sources = [(a.sidecar, "exact", a.clip)] if a.sidecar else []
    sources.append((a.clip, "key", None))
    for file, mode, clip in sources:
        if a.cold:
            evict(file)
            if clip:
                evict(clip)
        h = Helper(a.helper)
        first = h.ask(file, times[0], mode, a.width, a.height, clip)
        print(f"{os.path.basename(file)} {mode}{' cold' if a.cold else ''}: first picture (opens the file) "
              f"{first.get('ms', 0):.0f} ms, round trip {first['rt']:.0f} ms")
        print("  shuffled:   " + stats([h.ask(file, t, mode, a.width, a.height, clip) for t in times[1:]]))
        print("  along bar:  " + stats([h.ask(file, t, mode, a.width, a.height, clip) for t in walk]))
        print("  again:      " + stats([h.ask(file, t, mode, a.width, a.height, clip) for t in times[1:]]))
        # Two at once: the first is dropped (waiting, or stopped mid-decode)
        dropped = 0
        for t in times[1:11]:
            n1, a1, _ = h.ask(file, t + 0.37, mode, a.width, a.height, clip, wait=False)
            n2, a2, _ = h.ask(file, t + 0.77, mode, a.width, a.height, clip, wait=False)
            r1 = h.read(n1, a1)
            h.read(n2, a2)
            dropped += not r1["ok"] and r1.get("status") == "dropped"
        print(f"  two at once: {dropped}/10 of the first dropped")
        h.close()


if __name__ == "__main__":
    sys.exit(main())
