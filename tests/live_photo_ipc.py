#!/usr/bin/env python3
# The live-photo plugin in a player without a window (mpv --vo=null, driven over IPC): a live photo opens as its
# still, the hover over the Live icon plays its live part from the photo's own frame and round again while it stays,
# Space switches the same on and off (Esc off too), the run carries on into the photos taken right after it, round
# again from its start, and rests on the one it got to when stopped, a plain photo has no icon. Opened as a list from
# the Library (an .afplist.m3u8 as library/src/main.rs writes it, read by workspace/librarylist.lua): the photo at its
# place in it, the run's neighbours from the list (not the folder), and it rests on a photo of the list.
#
#   python3 tests/live_photo_ipc.py <folder of live photos taken in a row> [<a plain photo>]
#   python3 tests/live_photo_ipc.py --links <folder of a project's photo links (.afplink.jpg) to one clip, in a row>
#   AFP_PHOTO=<afp-photo built from macos/photo.swift> python3 tests/live_photo_ipc.py --hdr <a photo with a gain map>
#   python3 tests/live_photo_ipc.py --space <a clip> [<a moment> …]   Space pauses and plays them as before (it's the
#                                    live part's switch only while a live photo is shown)
#
# HDR (macOS): the photo's HDR rendition (afp-photo --hdr-still, into a scratch AFP_HOME) shown in place of its SDR
# image, labelled PQ BT.2020 for the VO, the target PQ; an SDR screen (user-data/dlogm/screen sdr) gets the photo's own
# image and the target back; the live part plays its own video, not labelled.
#
# Photo links: with the workspace plugin too (it opens a link as its clip's part around the frame): the link opens
# paused at its frame, the run of the links after it plays as one part and rests on the last at its frame, and Open
# the whole clip (open-original) opens the clip paused at that frame, its live part marked A-B.
#
# MPV=<mpv> (default: mpv on the PATH; the app's: …/ActionFootagePlayer.app/Contents/Helpers/mpv). Only this plugin is
# loaded, from the repository. The folder: Live Photos (HEIC + .mov) or Motion Photos (.MP.jpg), e.g. the player's own
# export of a clip at 2.0, 2.9 and 3.8 s, or an iPhone's burst.

import json, os, socket, subprocess, sys, tempfile, time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MPV = os.environ.get("MPV", "mpv")
SOCK = "/tmp/afp-live-photo-test.sock"
PHOTO = (".heic", ".jpg", ".jpeg")

failures = checks = 0
def expect(what, ok, detail=""):
    global failures, checks
    checks += 1
    if not ok:
        failures += 1
        print(f"FAIL {what} {detail}")
    else:
        print(f"ok   {what} {detail}")

# The workspace plugin's list reader alone (the rest of that plugin needs the app's tools)
LISTER = """package.path = %r .. "/mpv/scripts/workspace/?.lua;" .. package.path
require('librarylist').install(require('mp'))
"""

class Player:
    def __init__(self, path, workspace=False):
        if os.path.exists(SOCK): os.remove(SOCK)
        lister = os.path.join(tempfile.mkdtemp(), "librarylist_only.lua")
        open(lister, "w").write(LISTER % REPO)
        self.p = subprocess.Popen([MPV, "--no-config", "--vo=null", "--ao=null", "--idle=yes", "--autocreate-playlist=filter", "--hwdec=auto",
                                   f"--script={REPO_SCRIPTS or REPO + '/mpv/scripts/live-photo'}",
                                   *( [f"--script={REPO}/mpv/scripts/workspace"] if workspace else [f"--script={lister}"] ),
                                   f"--input-ipc-server={SOCK}", path],
                                  # (the workspace plugin keeps a cache in the folder it runs in without the app's)
                                  cwd=tempfile.mkdtemp(),
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                self.s = socket.socket(socket.AF_UNIX); self.s.connect(SOCK); break
            except OSError: time.sleep(0.1)
        self.f = self.s.makefile("rb"); self.n = 0
    def cmd(self, *a):
        self.n += 1; rid = self.n
        self.s.sendall((json.dumps({"command": list(a), "request_id": rid}) + "\n").encode())
        while True:
            m = json.loads(self.f.readline())
            if m.get("request_id") == rid: return m
    def get(self, p): return self.cmd("get_property", p).get("data")
    def photo(self): return self.get("user-data/afp/photo") or {}
    def dump(self):
        out = tempfile.mktemp(suffix=".json")
        self.cmd("script-message", "live-photo-dump", out)
        for _ in range(30):
            if os.path.exists(out): break
            time.sleep(0.05)
        time.sleep(0.1)
        d = json.load(open(out)); os.remove(out); return d
    def wait(self, cond, secs=5):
        end = time.time() + secs
        while time.time() < end:
            if cond(): return True
            time.sleep(0.05)
        return False
    def quit(self):
        try: self.cmd("quit")
        except Exception: pass
        self.p.wait(5)

def live_photos(folder):
    return sorted(os.path.join(folder, n) for n in os.listdir(folder) if n.lower().endswith(PHOTO))

def hdr(photo):
    # The plugin finds afp-photo in the gyroflow plugin's folder beside it: a scratch scripts folder with both
    d = tempfile.mkdtemp()
    os.symlink(f"{REPO}/mpv/scripts/live-photo", os.path.join(d, "live-photo"))
    os.makedirs(os.path.join(d, "gyroflow"))
    os.symlink(os.path.abspath(os.environ["AFP_PHOTO"]), os.path.join(d, "gyroflow", "afp-photo"))
    os.environ["AFP_HOME"] = tempfile.mkdtemp()
    global REPO_SCRIPTS
    REPO_SCRIPTS = os.path.join(d, "live-photo")
    p = Player(photo)
    track = lambda: next((t for t in p.get("track-list") if t.get("title") == "HDR"), None)
    expect("HDR: its rendition shown", p.wait(lambda: track() is not None and track().get("selected"), 10), str(track()))
    out = lambda k: (p.get("video-out-params") or {}).get(k)
    expect("HDR: PQ BT.2020 to the VO", p.wait(lambda: out("gamma") == "pq" and out("primaries") == "bt.2020", 5), f"{out('gamma')} {out('primaries')}")
    expect("HDR: target PQ", p.get("target-trc") == "pq" and p.get("target-colorspace-hint") is True, f"{p.get('target-trc')} {p.get('target-colorspace-hint')}")
    if p.photo().get("live"):
        p.cmd("script-message", "live-photo-view", "play")
        expect("HDR: the live part, its own video", p.wait(lambda: not (track() or {}).get("selected") and not any(f.get("label") == "live-photo-hdr" for f in p.get("vf")), 4))
        p.cmd("script-message", "live-photo-view", "stop")
        expect("HDR: back to the rendition", p.wait(lambda: (track() or {}).get("selected"), 4))
    p.cmd("set_property", "user-data/dlogm/screen", "sdr")
    expect("SDR screen: the photo's own image", p.wait(lambda: not (track() or {}).get("selected") and out("gamma") != "pq", 4), f"{out('gamma')}")
    expect("SDR screen: the target back", p.get("target-trc") != "pq", str(p.get("target-trc")))
    p.quit()

REPO_SCRIPTS = None

def links(folder):
    files = sorted(os.path.join(folder, n) for n in os.listdir(folder) if n.lower().endswith(".afplink.jpg"))
    p = Player(files[0], workspace=True)
    expect("link: a live photo", p.wait(lambda: p.photo().get("kind") == "link" and (p.get("user-data/workspace") or {}).get("photo")), str(p.photo()))
    span = p.photo().get("span")
    ws = p.get("user-data/workspace") or {}
    t = ws["photo"]["t"]
    expect("link: its clip's part, paused at the frame", p.wait(lambda: p.get("pause") is True and abs((p.get("time-pos") or -9) - (t - span[0])) < 0.05),
           f"pos {p.get('time-pos')} want {t - span[0]}")
    expect("link: the part is the clip's (offset)", abs(ws.get("offset", -1) - span[0]) < 1e-6, str(ws.get("offset")))
    run = [os.path.basename(c["path"]) for c in p.dump().get("chain", [])]
    print("     link run:", " -> ".join(run), "part", span)
    seen = []
    p.cmd("script-message", "live-photo-view", "play")
    expect("link: plays from its frame", p.wait(lambda: p.photo().get("playing") is True and p.get("pause") is False, 3)
           and (p.get("time-pos") or -9) >= t - span[0] - 0.1, f"pos {p.get('time-pos')} frame {t - span[0]}")
    end = time.time() + 3 * (span[1] - span[0]) + 10
    lowest_after_end, reached_end = None, False
    while time.time() < end:
        ph = p.photo()
        at = ph.get("at")
        if at and (not seen or seen[-1] != at): seen.append(at)
        pos = p.get("time-pos") or 0
        if pos > span[1] - span[0] - 0.5: reached_end = True
        if reached_end and pos < 1.0: lowest_after_end = pos; break
        time.sleep(0.05)
    expect("link: the run played through", [os.path.basename(s) for s in seen[:len(run)]] == run[run.index(os.path.basename(seen[0])):] if seen else False,
           str([os.path.basename(s) for s in seen]))
    expect("link: round again from the part's start", lowest_after_end is not None and p.photo().get("playing") is True, f"pos {lowest_after_end}")
    got_to = os.path.basename(p.photo().get("at") or "")
    p.cmd("script-message", "live-photo-view", "stop")
    expect("link: stopped, rests on the link it got to at its frame", p.wait(lambda: os.path.basename(p.get("path") or "") == got_to and p.get("pause") is True
                                                         and (p.get("user-data/workspace") or {}).get("photo") is not None, 6), f"{p.get('path')} want {got_to}")
    last = (p.get("user-data/workspace") or {}).get("photo") or {}
    p.cmd("script-message-to", "workspace", "open-original")
    expect("whole clip: the clip", p.wait(lambda: (p.get("path") or "").lower().endswith((".mp4", ".mov")), 6), str(p.get("path")))
    expect("whole clip: at the frame, paused", p.wait(lambda: p.get("pause") is True and abs((p.get("time-pos") or -9) - last.get("t", -1)) < 0.05, 4),
           f"pos {p.get('time-pos')} want {last.get('t')}")
    expect("whole clip: the live part marked", abs((p.get("ab-loop-a") or -9) - last.get("in", 0)) < 0.01 and abs((p.get("ab-loop-b") or -9) - last.get("out", 0)) < 0.01,
           f"{p.get('ab-loop-a')}–{p.get('ab-loop-b')}")
    expect("whole clip: not a photo", not p.photo())
    p.quit()

def space(paths):
    for path in paths:
        p = Player(path, workspace=True)
        p.wait(lambda: (p.get("duration") or 0) > 0, 8)
        p.cmd("set_property", "pause", False); time.sleep(0.6)
        before = p.get("pause")
        p.cmd("keypress", "SPACE"); time.sleep(0.5)
        after = p.get("pause")
        p.cmd("keypress", "SPACE"); time.sleep(0.5)
        again = p.get("pause")
        expect(os.path.basename(path) + ": Space pauses and plays as before", before is False and after is True and again is False,
               f"{before} -> {after} -> {again}")
        expect(os.path.basename(path) + ": not a photo", not p.photo())
        p.quit()

def main():
    if sys.argv[1] == "--space":
        space(sys.argv[2:])
        print(f"{checks} checks, {failures} failed")
        sys.exit(1 if failures else 0)
    if sys.argv[1] == "--hdr":
        hdr(sys.argv[2])
        print(f"{checks} checks, {failures} failed")
        sys.exit(1 if failures else 0)
    if sys.argv[1] == "--links":
        links(sys.argv[2])
        print(f"{checks} checks, {failures} failed")
        sys.exit(1 if failures else 0)
    folder = sys.argv[1]
    plain = sys.argv[2] if len(sys.argv) > 2 else None
    photos = live_photos(folder)
    first = photos[0]

    # Opened: the still, paused, staying; the .mov out of the playlist
    p = Player(first)
    expect("opens as a photo", p.wait(lambda: p.photo().get("live") is True), str(p.photo()))
    p.wait(lambda: p.get("track-list/count") and any(t.get("external") for t in p.get("track-list")), 5)
    movs = lambda: [os.path.basename(e["filename"]) for e in p.get("playlist") if e["filename"].lower().endswith(".mov")]
    # (taken out a few at a time after the photo shows)
    expect("the .movs aren't in the playlist", p.wait(lambda: not movs(), 5), str(movs()))
    expect("the still stays", p.get("image-display-duration") == "inf" or p.get("image-display-duration") is None
           or str(p.get("image-display-duration")) == "inf", str(p.get("image-display-duration")))
    image_vid = p.get("vid")
    expect("paused on the picture", p.get("pause") is True)

    # The hover: the icon laid out as on a 1440x900 window, the mouse on it, then off it
    p.cmd("script-message", "live-photo-layout", "1440", "900", "1", "0", "0", "0", "0")
    icon = p.dump().get("icon")
    expect("the Live icon is there", bool(icon), str(icon))
    p.cmd("mouse", 700, 450)
    time.sleep(0.2)
    expect("away from the icon: still", p.photo().get("playing") is False)
    p.cmd("mouse", int(icon["x0"]) + 5, int(icon["y0"]) + 5)
    expect("hovered: plays", p.wait(lambda: p.photo().get("playing") is True))
    still_t = (p.dump().get("photo") or {}).get("still_t") or 0
    expect("hovered: its video", p.wait(lambda: p.get("vid") != image_vid and (p.get("time-pos") or 0) > 0.05), f"vid {p.get('vid')} pos {p.get('time-pos')}")
    pos0 = p.get("time-pos") or -9
    expect("hovered: from the photo's own frame", still_t - 0.05 <= pos0 <= still_t + 0.6, f"pos {pos0} still {still_t}")
    expect("hovered: its sound", p.get("aid") not in (False, None, "no"), str(p.get("aid")))
    p.cmd("mouse", 700, 450)
    expect("left: the still again", p.wait(lambda: p.photo().get("playing") is False and p.get("vid") == image_vid and p.get("pause") is True))
    expect("left: on the same photo", os.path.basename(p.get("path")) == os.path.basename(first), p.get("path"))

    # Space: a switch (a tap played a frame and jumped back before), Esc off too
    p.cmd("keypress", "SPACE")
    expect("Space: plays", p.wait(lambda: p.photo().get("playing") is True))
    time.sleep(0.8)
    expect("Space: still playing after the tap", p.photo().get("playing") is True)
    p.cmd("keypress", "SPACE")
    expect("Space again: the still", p.wait(lambda: p.photo().get("playing") is False and p.get("vid") == image_vid))
    p.cmd("keypress", "SPACE")
    p.wait(lambda: p.photo().get("playing") is True)
    p.cmd("keypress", "ESC")
    expect("Esc: the still", p.wait(lambda: p.photo().get("playing") is False and p.get("vid") == image_vid))

    # The run: on, it goes through the photos taken right after, then round again from the first; stopped, it rests on
    # the photo it got to
    chain = [c["path"] for c in p.dump().get("chain", [])]
    print("     run:", " -> ".join(os.path.basename(c) for c in chain))
    seen = []
    p.cmd("script-message", "live-photo-view", "play")
    end = time.time() + 4 * len(chain) + 8
    while time.time() < end:
        ph = p.photo()
        at = ph.get("at")
        if at and (not seen or seen[-1] != at): seen.append(at)
        if len(seen) > len(chain): break
        time.sleep(0.05)
    names = [os.path.basename(c) for c in chain]
    expect("the run played through", [os.path.basename(s) for s in seen[:len(chain)]] == names, str([os.path.basename(s) for s in seen]))
    expect("and round again from its start", len(seen) > len(chain) and os.path.basename(seen[len(chain)]) == names[0]
           and p.photo().get("playing") is True, str([os.path.basename(s) for s in seen]))
    if len(chain) > 1:
        p.wait(lambda: os.path.basename(p.photo().get("at") or "") == names[1], 6)
    got_to = os.path.basename(p.photo().get("at") or "")
    p.cmd("script-message", "live-photo-view", "stop")
    expect("stopped: rests on the photo it got to", p.wait(lambda: os.path.basename(p.get("path") or "") == got_to
                                                         and p.photo().get("playing") is False, 5), f"{p.get('path')} want {got_to}")
    p.quit()

    # A list from the Library: the first photo and the third only (the second left out, as a filter would): the run goes
    # from the first to the third when their live parts overlap, and stays in the list
    if len(photos) >= 3:
        d = tempfile.mkdtemp()
        lst = os.path.join(d, "1.afplist.m3u8")
        items = [photos[0], photos[2]]
        open(lst, "w").write("#EXTM3U\n#PLAYLIST:Test\n#AFP-START:0\n" + "".join(f"#EXTINF:-1,{os.path.basename(x)}\n{x}\n" for x in items))
        r = Player(lst)
        expect("list: the photo opened, as a still", r.wait(lambda: r.photo().get("live") is True and os.path.basename(r.get("path") or "") == os.path.basename(items[0])),
               str(r.get("path")))
        expect("list: the playlist is the list", r.get("playlist-count") == 2, str(r.get("playlist-count")))
        run = [os.path.basename(c["path"]) for c in r.dump().get("chain", [])]
        print("     list run:", " -> ".join(run))
        expect("list: neighbours from the list", os.path.basename(photos[1]) not in run, str(run))
        r.cmd("script-message", "live-photo-view", "play")
        r.wait(lambda: r.photo().get("playing") is True, 3)
        r.wait(lambda: os.path.basename(r.photo().get("at") or "") == run[-1], 4 * len(run) + 5)
        r.cmd("script-message", "live-photo-view", "stop")
        expect("list: rests on a photo of the list", r.wait(lambda: os.path.basename(r.get("path") or "") == run[-1], 5)
               and r.get("playlist-count") == 2, f"{r.get('path')} {r.get('playlist-count')}")
        r.quit()

    if plain:
        q = Player(plain)
        expect("a plain photo: a photo, not live", q.wait(lambda: q.photo().get("live") is False), str(q.photo()))
        q.cmd("script-message", "live-photo-layout", "1440", "900", "1", "0", "0", "0", "0")
        expect("a plain photo: no icon", not q.dump().get("icon"))
        q.quit()

    print(f"{checks} checks, {failures} failed")
    sys.exit(1 if failures else 0)

if __name__ == "__main__":
    main()
