#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# jellyfin.py: a project's moments and clips in Jellyfin, as the player plays them (tools/stream/README.md, "In
# Jellyfin"). afp-stream renders each moment and clip preview the way the player shows it (stabilized, in color) and
# lists them in its catalog; this writes a folder Jellyfin reads as a Home Videos library, a .strm (the stream's URL)
# per item with a .nfo (title, date, length, what it is) and its cover, in the trips the clip previews are sorted
# into:
#
#   <out>/Moments/<trip>/<moment>.strm        a moment: its subfolder in _Moments/, else its clip's trip, else "No trip"
#   <out>/Clips/<trip>/<clip>.strm            a clip preview (the whole clip): its folder in _Clips/
#
# --scope says which of the two (moments, clips or all); what was written outside it is removed.
#
# Nothing is written beside the footage: only <out>, whose files it lists in <out>/.afp-jellyfin.json and removes
# when their item goes (a moment deleted, a clip preview deleted as not interesting). Removing <out> removes it all.
# With --jellyfin and --key-file, the library whose folder is <out> is refreshed after a change (only that library).
#
#   jellyfin.py --catalog http://127.0.0.1:8099/catalog.json --stream http://192.168.50.186:8099 --out /out \
#               [--jellyfin http://127.0.0.1:8899 --key-file /run/key] [--names en|ru] [--scope moments|clips|all] [--every 120]

import argparse, html, json, os, re, sys, time, urllib.parse, urllib.request

NAMES = {
    "en": {"moments": "Moments", "clips": "Clips", "none": "No trip", "moment": "A moment of {clip}, {a}–{b} s.",
           "clip": "The whole clip {clip}, {d}.", "stab": "Stabilized", "nostab": "Not stabilized", "hdr": "HDR (HLG)", "s": "s"},
    "ru": {"moments": "Моменты", "clips": "Клипы", "none": "Без поездки", "moment": "Момент клипа {clip}, {a}–{b} с.",
           "clip": "Весь клип {clip}, {d}.", "stab": "Стабилизирован", "nostab": "Без стабилизации", "hdr": "HDR (HLG)", "s": "с"},
}
MANIFEST = ".afp-jellyfin.json"


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


def fetch(url, key=None, method="GET"):
    req = urllib.request.Request(url, method=method, data=b"" if method == "POST" else None,
                                 headers={"Authorization": f'MediaBrowser Token="{key}"'} if key else {})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def safe(name):
    """A file or folder name: no path separators or characters Windows refuses, no leading/trailing dots or spaces"""
    name = re.sub(r'[\x00-\x1f/\\:*?"<>|]', "-", name).strip(" .")
    return name[:180] or "-"


def hms(seconds, unit="s"):
    s = int(round(seconds))
    return f"{s // 60}:{s % 60:02d}" if s >= 60 else f"{s} {unit}"


def nfo(item, t):
    start = item.get("start")  # YYYY-MM-DDTHH:MM:SS, the camera's clock
    d = item["duration"]
    if item["kind"] == "moment":
        plot = t["moment"].format(clip=item["clip"], a=f"{item['in']:.1f}", b=f"{item['in'] + d:.1f}")
    else:
        plot = t["clip"].format(clip=item["clip"], d=hms(d, t["s"]))
    plot += " " + (t["stab"] if item["stabilized"] else t["nostab"]) + (", " + t["hdr"] if item["look"] == "hdr" else "") + "."
    e = lambda v: html.escape(str(v), quote=False)
    lines = ['<?xml version="1.0" encoding="utf-8" standalone="yes"?>', "<movie>", f"  <title>{e(item['label'])}</title>"]
    if start:
        lines += [f"  <sorttitle>{e(start)}</sorttitle>", f"  <premiered>{start[:10]}</premiered>", f"  <year>{start[:4]}</year>",
                  f"  <dateadded>{start.replace('T', ' ')}</dateadded>"]
    lines += [f"  <plot>{e(plot)}</plot>", f"  <runtime>{max(1, round(d / 60))}</runtime>",
              "  <fileinfo>", "    <streamdetails>", "      <video>", "        <codec>hevc</codec>",
              f"        <durationinseconds>{max(1, round(d))}</durationinseconds>", "      </video>", "    </streamdetails>",
              "  </fileinfo>", f"  <uniqueid type=\"afp\" default=\"true\">{e(item['id'])}</uniqueid>", "</movie>", ""]
    return "\n".join(lines)


def label(item):
    """What Jellyfin shows: a moment's name; a clip by its time ("2026-10-23 14:48")"""
    if item["kind"] == "moment":
        return item["title"]
    return item["start"][:16].replace("T", " ") if item.get("start") else item["name"]


SCOPES = {"moments": ("moment",), "clips": ("preview",), "all": ("moment", "preview")}


def tree(items, t, scope="all"):
    """{relative path: item} for each .strm: moments in their folder or their clip's trip, clips in theirs; only the
    kinds in scope (a moment still finds its trip from its clip's preview)"""
    trip_of = {}
    for it in items:
        if it["kind"] == "preview":
            trip_of[(it["project"], it["clip"])] = it["folder"]
    out = {}
    for it in sorted(items, key=lambda i: (i.get("start") or "", i["name"])):
        if it["kind"] not in SCOPES[scope]:
            continue
        if it["kind"] == "moment":
            folder = it["folder"] or trip_of.get((it["project"], it["clip"])) or t["none"]
            top = t["moments"]
        else:
            folder = it["folder"] or t["none"]
            top = t["clips"]
        parts = [top] + [safe(p) for p in folder.split("/") if p]
        base = "/".join(parts + [safe(it["name"])])
        rel, n = base, 2
        while rel + ".strm" in out:
            rel, n = f"{base} {n}", n + 1
        it = dict(it, label=label(it))
        out[rel + ".strm"] = it
    return out


def write(path, data):
    """Written only when it differs (a library refresh then has nothing to do), whole: a temporary file renamed"""
    data = data.encode() if isinstance(data, str) else data
    try:
        with open(path, "rb") as f:
            if f.read() == data:
                return False
    except OSError:
        pass
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "wb") as f:
        f.write(data)
    os.replace(tmp, path)
    return True


def run(a):
    t = NAMES[a.names]
    cat = json.loads(fetch(a.catalog))
    items = cat["items"]
    if cat.get("pending"):
        log(f"{cat['pending']} items not ready yet in afp-stream: next time")
    stream = a.stream.rstrip("/")
    changed = 0
    wanted = set()
    folders = {}
    for rel, it in tree(items, t, a.scope).items():
        base = os.path.join(a.out, rel[:-5])
        url = f"{stream}/clip/{it['id']}/index.m3u8"
        changed += write(base + ".strm", url + "\n")
        changed += write(base + ".nfo", nfo(it, t))
        wanted.update([base + ".strm", base + ".nfo"])
        poster = base + "-poster.jpg"
        try:
            changed += write(poster, fetch(f"{a.stream_local.rstrip('/')}/poster/{it['id']}.jpg"))
        except Exception as e:
            log("no poster for", rel, e)
        if os.path.exists(poster):
            wanted.add(poster)
            # A folder's picture: its first item's (the earliest)
            d = os.path.dirname(base)
            while d.startswith(a.out) and d != a.out and d not in folders:
                folders[d] = poster
                d = os.path.dirname(d)
    for d, poster in folders.items():
        with open(poster, "rb") as f:
            changed += write(os.path.join(d, "folder.jpg"), f.read())
        wanted.add(os.path.join(d, "folder.jpg"))
    # What was written before and isn't wanted now goes, then the folders left empty
    try:
        with open(os.path.join(a.out, MANIFEST)) as f:
            before = set(os.path.join(a.out, p) for p in json.load(f))
    except (OSError, ValueError):
        before = set()
    for p in sorted(before - wanted):
        try:
            os.remove(p)
            changed += 1
        except OSError:
            pass
    for d, _, _ in sorted(os.walk(a.out), key=lambda w: -len(w[0])):
        if d != a.out and not os.listdir(d):
            os.rmdir(d)
    write(os.path.join(a.out, MANIFEST), json.dumps(sorted(os.path.relpath(p, a.out) for p in wanted), ensure_ascii=False, indent=0))
    log(f"{len(items)} items, {a.scope} in Jellyfin, {changed} files changed")
    if changed and a.jellyfin and a.key_file:
        refresh(a)


def refresh(a):
    """The library whose folder is a.jellyfin_path (else a.out) scanned again: only it"""
    key = open(a.key_file).read().strip()
    base = a.jellyfin.rstrip("/")
    want = (a.jellyfin_path or a.out).rstrip("/")
    for lib in json.loads(fetch(f"{base}/Library/VirtualFolders", key)):
        if any(p.rstrip("/") == want for p in lib.get("Locations") or []):
            q = urllib.parse.urlencode({"Recursive": "true", "MetadataRefreshMode": "Default", "ImageRefreshMode": "Default"})
            fetch(f"{base}/Items/{lib['ItemId']}/Refresh?{q}", key, "POST")
            log("refreshed", lib["Name"])
            return
    log("no library has", want)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--catalog", default="http://127.0.0.1:8099/catalog.json")
    p.add_argument("--stream", required=True, help="afp-stream as Jellyfin's clients reach it, for the .strm files")
    p.add_argument("--stream-local", default="http://127.0.0.1:8099", help="afp-stream as reached from here, for the covers")
    p.add_argument("--out", required=True)
    p.add_argument("--jellyfin", help="Jellyfin's URL, to refresh the library after a change")
    p.add_argument("--jellyfin-path", help="the folder as Jellyfin's library has it (its container's path), if not --out")
    p.add_argument("--key-file", help="a file with a Jellyfin API key")
    p.add_argument("--names", default="en", choices=sorted(NAMES))
    p.add_argument("--scope", default="all", choices=sorted(SCOPES), help="what goes to Jellyfin: moments, clips (clip previews) or all; what's outside it is removed")
    p.add_argument("--every", type=float, default=0, help="seconds between runs; 0: once")
    a = p.parse_args()
    a.out = os.path.abspath(a.out)
    while True:
        try:
            run(a)
        except Exception as e:
            log("failed:", e)
            if not a.every:
                sys.exit(1)
        if not a.every:
            break
        time.sleep(a.every)
