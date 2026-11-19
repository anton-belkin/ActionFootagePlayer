#!/usr/bin/env python3
# The live-photo plugin in a player without a window (mpv --vo=null, driven over IPC): a live photo opens as its
# still, the hover over the Live icon and Space held play its live part and give the still back, the run carries on
# into the photos taken right after it and rests on the one it got to, a plain photo has no icon.
#
#   python3 tests/live_photo_ipc.py <folder of live photos taken in a row> [<a plain photo>]
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

class Player:
    def __init__(self, path):
        if os.path.exists(SOCK): os.remove(SOCK)
        self.p = subprocess.Popen([MPV, "--no-config", "--vo=null", "--ao=null", "--idle=yes", "--autocreate-playlist=filter",
                                   f"--script={REPO}/mpv/scripts/live-photo", f"--input-ipc-server={SOCK}", path],
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

def main():
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
    expect("hovered: its video", p.wait(lambda: p.get("vid") != image_vid and (p.get("time-pos") or 0) > 0.3), f"vid {p.get('vid')} pos {p.get('time-pos')}")
    expect("hovered: its sound", p.get("aid") not in (False, None, "no"), str(p.get("aid")))
    p.cmd("mouse", 700, 450)
    expect("left: the still again", p.wait(lambda: p.photo().get("playing") is False and p.get("vid") == image_vid and p.get("pause") is True))
    expect("left: on the same photo", os.path.basename(p.get("path")) == os.path.basename(first), p.get("path"))

    # Space held
    p.cmd("keydown", "SPACE")
    expect("Space held: plays", p.wait(lambda: p.photo().get("playing") is True))
    p.cmd("keyup", "SPACE")
    expect("Space let go: the still", p.wait(lambda: p.photo().get("playing") is False and p.get("vid") == image_vid))

    # The run: held to its end, it goes through the photos taken right after and rests on the last one
    chain = [c["path"] for c in p.dump().get("chain", [])]
    print("     run:", " -> ".join(os.path.basename(c) for c in chain))
    seen = []
    p.cmd("script-message", "live-photo-view", "play")
    end = time.time() + 4 * len(chain) + 5
    while time.time() < end:
        ph = p.photo()
        at = ph.get("at")
        if at and (not seen or seen[-1] != at): seen.append(at)
        if seen and not ph.get("playing"): break
        time.sleep(0.05)
    expect("the run played through", [os.path.basename(s) for s in seen] == [os.path.basename(c) for c in chain],
           str([os.path.basename(s) for s in seen]))
    expect("rests on the photo it got to", p.wait(lambda: os.path.basename(p.get("path") or "") == os.path.basename(chain[-1])
                                                and p.photo().get("playing") is False, 5), str(p.get("path")))
    p.quit()

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
