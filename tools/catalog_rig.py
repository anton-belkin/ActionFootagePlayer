#!/usr/bin/env python3
# The catalog's rig on the NAS's side (docs/library-management.md §9): makes a test project fast (on the NAS's own
# disk), changes it as Finder on another computer would, and appends journal lines as another computer.
#   make <base> <days> <per>          <base>/proj (Project.afproject, Generated/... moments) and <base>/footage
#   finder <base> <seed> <n>          n random changes: files added, removed, renamed, folders moved and copied
#   append <base> <computer> <n>      n journal lines as <computer> (a note each), into a new segment
#   touch <dir>                       a file made in a folder (its time changes), for the visibility test
import hashlib, json, os, random, shutil, sys, time

def moment(h, clip, n):
    return json.dumps({"kind": "moment", "hash": h, "clip": clip, "in": n}, separators=(",", ":"))

def make(base, days, per):
    p, f = os.path.join(base, "proj"), os.path.join(base, "footage")
    os.makedirs(p, exist_ok=True); os.makedirs(f, exist_ok=True)
    open(os.path.join(p, "Project.afproject"), "w").write('{"format":"actionfootage-project"}')
    open(os.path.join(f, "footage.afplocation"), "w").write("{}")
    for d in range(days):
        fd = os.path.join(f, "Day%04d" % d); os.makedirs(fd, exist_ok=True)
        md = os.path.join(p, "Generated", "2026-11-%02d Moments" % (1 + d % 28), "Day%04d" % d); os.makedirs(md, exist_ok=True)
        for c in range(per):
            clip = "Day%04d/C%04d.mp4" % (d, c)
            open(os.path.join(f, clip), "w").write("clip %d %d " % (d, c) + "." * (c % 50))
            open(os.path.join(md, "C%04d m.mp4" % c), "w").write(moment("h%dx%d" % (d, c), clip, c))

def finder(base, seed, n):
    r = random.Random(seed)
    p = os.path.join(base, "proj")
    for i in range(n):
        # Hidden folders (the catalog's .afp) left alone, as Finder doesn't show them
        dirs = [os.path.join(dp, d) for dp, dn, fn in os.walk(p) if "/." not in dp for d in dn if not d.startswith(".")]
        files = [os.path.join(dp, x) for dp, dn, fn in os.walk(p) if "/." not in dp for x in fn if x.endswith(".mp4")]
        op = r.randrange(5)
        try:
            if op == 0 and dirs:
                open(os.path.join(r.choice(dirs), "finder%d-%d.mp4" % (seed, i)), "w").write(moment("hf%d" % r.randrange(9), "x", i))
            elif op == 1 and files: os.remove(r.choice(files))
            elif op == 2 and files:
                x = r.choice(files); os.rename(x, os.path.join(os.path.dirname(x), "renamed%d-%d.mp4" % (seed, i)))
            elif op == 3 and dirs:
                x = r.choice(dirs); dest = os.path.join(p, "Moved", "%d-%d" % (seed, i))
                if not dest.startswith(x + "/"): os.makedirs(os.path.dirname(dest), exist_ok=True); os.rename(x, dest)
            elif op == 4 and dirs:
                x = r.choice(dirs); shutil.copytree(x, os.path.join(p, "Copied%d-%d" % (seed, i)), copy_function=shutil.copy2)
        except (OSError, shutil.Error):
            pass

def frame(text):
    return ("%s %s\n" % (hashlib.sha256(text.encode()).hexdigest()[:8], text)).encode()

def append(base, comp, n):
    d = os.path.join(base, "proj", ".afp", "catalog", "journal", comp); os.makedirs(d, exist_ok=True)
    segs = sorted(int(x[:-4]) for x in os.listdir(d) if x.endswith(".log"))
    seq = 0
    for s in segs:
        for line in open(os.path.join(d, "%012d.log" % s), "rb").read().split(b"\n"):
            try: seq = max(seq, json.loads(line.split(b" ", 1)[1]).get("q", 0))
            except Exception: pass
    first = seq + 1
    with open(os.path.join(d, "%012d.log" % first), "ab") as out:
        out.write(frame(json.dumps({"computer": comp, "segment": first, "format": 1}, separators=(",", ":"))))
        for i in range(n):
            seq += 1
            rel = "Generated/from %s/%d.mp4" % (comp, seq)
            e = {"r": "p", "p": rel, "s": 10, "m": 1, "x": {"link": {"kind": "moment", "hash": "nas", "clip": "x"}}}
            out.write(frame(json.dumps({"q": seq, "l": 1000000000 + seq, "e": [e]}, separators=(",", ":"))))
            out.flush(); os.fsync(out.fileno())

if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "make": make(a[1], int(a[2]), int(a[3]))
    elif a[0] == "finder": finder(a[1], int(a[2]), int(a[3]))
    elif a[0] == "append": append(a[1], a[2], int(a[3]))
    elif a[0] == "touch": open(os.path.join(a[1], "touched-%d" % time.time_ns()), "w").write("x")
