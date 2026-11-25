#!/usr/bin/env python3
"""A list opened from Action Footage Library, played by a headless mpv with the workspace plugin
(mpv/scripts/workspace/librarylist.lua), driven over its JSON IPC as the Library's open would:

  - the list's items are the playlist, in its order, the item opened playing (not the first);
  - a moment in it plays as a moment (its original's part), Next and Previous walk the list;
  - a second list replaces the first and plays its own item;
  - a list of one stays one (not the item's folder);
  - a clip opened by itself (Finder, the command line) still gets its folder as the playlist.

    python3 tests/librarylist_ipc.py --mpv <mpv> [--afp-cut <afp-cut>] [--work <scratch dir>]

Needs ffmpeg. Without afp-cut (the player's, beside gyroflow-probe) the moment is left out. No window: --vo=null.
"""
import argparse, json, os, shutil, socket, subprocess, sys, tempfile, time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
failures = 0


def check(what, got, want):
    global failures
    ok = got == want
    if not ok:
        failures += 1
    print(("ok   " if ok else "FAIL ") + what + ("" if ok else f": got {got!r}, want {want!r}"))


def clip(path, seconds=3):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", f"testsrc=size=160x90:rate=25:duration={seconds}",
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", path], check=True)


def write_list(path, items, start, label):
    # As the Library writes it (library/src/playing.rs, list_m3u)
    lines = ["#EXTM3U", "#PLAYLIST:" + label, f"#AFP-START:{start}"]
    for title, p in items:
        lines += [f"#EXTINF:-1,{title}", p]
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


class Mpv:
    def __init__(self, mpv, sock, log, work):
        self.p = subprocess.Popen([mpv, "--no-config", "--idle=yes", "--vo=null", "--ao=null", "--msg-level=all=warn,workspace=info,cplayer=info",
                                   f"--input-ipc-server={sock}", "--autocreate-playlist=filter", "--keep-open=no",
                                   "--script=" + os.path.join(REPO, "mpv/scripts/workspace")],
                                  stdout=log, stderr=subprocess.STDOUT, cwd=work)  # (its caches there: ~~cache)
        for _ in range(100):
            try:
                self.s = socket.socket(socket.AF_UNIX)
                self.s.connect(sock)
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise SystemExit("mpv's IPC socket never came")
        self.buf, self.events, self.n = b"", [], 0

    def _lines(self, timeout):
        self.s.settimeout(timeout)
        try:
            data = self.s.recv(65536)
        except socket.timeout:
            return
        self.buf += data
        while b"\n" in self.buf:
            line, self.buf = self.buf.split(b"\n", 1)
            if line.strip():
                yield json.loads(line)

    def cmd(self, *args):
        self.n += 1
        rid = self.n
        self.s.sendall((json.dumps({"command": list(args), "request_id": rid}) + "\n").encode())
        until = time.time() + 10
        while time.time() < until:
            for m in self._lines(0.5):
                if m.get("request_id") == rid:
                    return m.get("data")
                if "event" in m:
                    self.events.append(m)
        raise SystemExit(f"no answer to {args}")

    def get(self, prop):
        return self.cmd("get_property", prop)

    def wait_playing(self, path, timeout=15):
        """Until `path` plays (its first frame), the events before dropped"""
        self.events = []
        until = time.time() + timeout
        while time.time() < until:
            if self.get("path") == path and self.get("time-pos") is not None:
                return True
            time.sleep(0.1)
        return False

    def quit(self):
        try:
            self.cmd("quit")
        except SystemExit:
            pass
        self.p.wait(10)


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--mpv", required=True)
    a.add_argument("--afp-cut")
    a.add_argument("--work")
    o = a.parse_args()
    work = o.work or tempfile.mkdtemp(prefix="afp-librarylist-")
    if os.path.exists(work):
        shutil.rmtree(work)
    proj = os.path.join(work, "Trip")
    os.makedirs(proj)
    with open(os.path.join(proj, "Project.afproject"), "w") as f:
        f.write('{"v": 4}\n')
    A, B, C, D = (os.path.join(proj, "Day 1", n) for n in ("A.MP4", "B.MP4", "C.MP4", "D.MP4"))
    for c in (A, B, C, D):
        clip(c)
    moment = None
    if o.afp_cut:
        # A moment of A's 1-2 s: a small video with the link in it, as the player's k makes it
        moment = os.path.join(proj, "_Moments", "Turtle at the wreck.afplink.mp4")
        clip(moment, 1)
        link = {"v": 1, "kind": "moment", "clip": "Day 1/A.MP4", "size": os.path.getsize(A), "in": 1.0, "out": 2.0}
        subprocess.run([o.afp_cut, "--set-key", moment, "ch.actionfootageplayer.link", json.dumps(link)], check=True,
                       stdout=subprocess.DEVNULL)
    lists = os.path.join(work, "lists")
    os.makedirs(lists)

    sock = f"/tmp/afp-ll-{os.getpid()}.sock"
    log = open(os.path.join(work, "mpv.log"), "w")
    m = Mpv(o.mpv, sock, log, work)
    try:
        # 1. The Library's list: C, the moment, A, B; the moment opened (the second)
        first = [("C", C)] + ([("Turtle at the wreck", moment)] if moment else []) + [("A", A), ("B", B)]
        at = 1
        l1 = os.path.join(lists, "1.afplist.m3u8")
        write_list(l1, first, at, "Originals › Day 1 · filtered")
        m.cmd("loadfile", l1, "replace")
        check("opened item plays", m.wait_playing(first[at][1]), True)
        check("playlist is the list", [e["filename"] for e in m.get("playlist")], [p for _, p in first])
        check("titles", [e.get("title") for e in m.get("playlist")], [t for t, _ in first])
        check("position", m.get("playlist-pos"), at)
        check("label for the panel", (m.get("user-data/library-list") or {}).get("label"), "Originals › Day 1 · filtered")
        if moment:
            check("moment plays as a moment", (m.get("user-data/workspace/moment") or {}).get("name"), "Turtle at the wreck")
            check("its part of the original", str(m.get("stream-open-filename")).startswith("edl://"), True)
            check("its length", round(m.get("duration") or 0), 1)
        m.cmd("playlist-next")
        check("next", m.wait_playing(first[at + 1][1]), True)
        check("next position", m.get("playlist-pos"), at + 1)
        m.cmd("playlist-prev")
        m.cmd("playlist-prev")
        check("previous twice", m.wait_playing(first[at - 1][1]), True)
        check("still the list", m.get("playlist-count"), len(first))

        # 2. Opened again from the Library (another filter): its list in place of the first, its item playing
        second = [("D", D), ("B", B)]
        l2 = os.path.join(lists, "2.afplist.m3u8")
        write_list(l2, second, 1, "Search: B")
        m.cmd("loadfile", l2, "replace")
        check("second open plays its item", m.wait_playing(B), True)
        check("second list replaces", [e["filename"] for e in m.get("playlist")], [D, B])
        check("second position", m.get("playlist-pos"), 1)

        # 3. A list of one: that one, not its folder
        l3 = os.path.join(lists, "3.afplist.m3u8")
        write_list(l3, [("C", C)], 0, "Search: C")
        m.cmd("loadfile", l3, "replace")
        check("one plays", m.wait_playing(C), True)
        time.sleep(1)
        check("list of one stays one", [e["filename"] for e in m.get("playlist")], [C])

        # 4. A clip opened by itself (Finder, the command line): its folder, as before
        m.cmd("loadfile", B, "replace")
        check("finder's clip plays", m.wait_playing(B), True)
        check("finder's: the folder", sorted(e["filename"] for e in m.get("playlist")), sorted([A, B, C, D]))
    finally:
        m.quit()
        log.close()
        if os.path.exists(sock):
            os.unlink(sock)
    # mpv's own account of what it loaded (it doesn't name the list file itself): the item opened first, nothing
    # before it in the list (a clip on a NAS takes seconds to open)
    with open(os.path.join(work, "mpv.log")) as f:
        played = [l.split("Playing: ", 1)[1].strip() for l in f if "Playing: " in l]
    check("loaded first: the item opened", played[:1], [first[at][1]])
    print(f"{failures} failed; mpv's log: {os.path.join(work, 'mpv.log')}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
