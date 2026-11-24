#!/usr/bin/env python3
"""The seek bar's picture end to end, without a window: a headless mpv (--vo=null) runs the side panel's scrub.lua
with afp-scrub, driven over mpv's JSON IPC as the bar drives it (want: the mouse over the track at a time; hide: it
left), and checks what's asked of the helper and what comes on screen (user-data/afp-scrub/shown, the overlay-add
done in mpv):

  - a clip with a sidecar (.LRF): its exact frame at the mouse's time; moving along, the next ones from the decoder
    going on (walk) or from memory (cache), each in a few ms;
  - leaving the track takes the picture away; an answer coming late isn't shown;
  - a moment's link plays its original's part: the original's sidecar at the original's time;
  - a clip without a sidecar: its keyframe at or before the time.

    python3 tests/scrub_ipc.py --mpv <mpv> --helper <afp-scrub> --clip <with .LRF> [--clip2 <without>]
        [--shots <dir>]   the bar as drawn (the panel's dump) over the clip's frame, with the helper's picture:
                          shots/after.png, shots/after-keyframe.png (with --clip2) and, from the side panel at
                          --before (a folder, default: git's HEAD~ of mpv/scripts/side-panel... see below)
                          shots/before.png
        [--before <ref>]  the git ref whose side panel draws the before picture (default origin/main)
"""
import argparse, json, os, shutil, socket, subprocess, sys, tempfile, time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PANEL = os.path.join(REPO, "mpv", "scripts", "side-panel")
W, H = 1600, 900
failures = 0

PROBE = r'''
-- The bar's part of the side panel, alone: scrub.lua as main.lua drives it
package.path = package.path .. ";%s/?.lua"
local mp = require 'mp'
local utils = require 'mp.utils'
local J = setmetatable({}, { __index = function(_, k) return function() end end })
local P = require('scrub').new(mp, utils, J, mp.get_property("platform"))
mp.register_script_message("want", function(t, mx) P.want(tonumber(t), tonumber(mx), 0, %d, %d, 1) end)
mp.register_script_message("hide", function() P.hide() end)
mp.register_script_message("forget", function() P.forget_clip() end)
mp.register_script_message("warm", function() P.warm() end)
mp.register_script_message("scrub", function(line) P.reply(line) end)
mp.register_event("start-file", function() P.forget_clip() end)
mp.register_event("shutdown", function() P.quit() end)
mp.set_property_native("user-data/probe", { available = P.available })
''' % (PANEL.replace("\\", "/"), W, H - 66)


def check(what, got, want=True):
    global failures
    ok = got == want
    if not ok:
        failures += 1
    print(("ok   " if ok else "FAIL ") + what + ("" if ok else f": got {got!r}, want {want!r}"))


class Mpv:
    def __init__(self, mpv, sock, log, args):
        self.p = subprocess.Popen([mpv, "--no-config", "--idle=yes", "--ao=null", "--vo=null", "--pause",
                                   "--msg-level=all=warn,scrubprobe=v,side_panel=info", f"--input-ipc-server={sock}"] + args,
                                  stdout=log, stderr=subprocess.STDOUT)
        for _ in range(100):
            try:
                self.s = socket.socket(socket.AF_UNIX)
                self.s.connect(sock)
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise SystemExit("mpv's IPC socket never came")
        self.buf, self.n = b"", 0

    def cmd(self, *args):
        self.n += 1
        rid = self.n
        self.s.sendall((json.dumps({"command": list(args), "request_id": rid}) + "\n").encode())
        until = time.time() + 10
        while time.time() < until:
            self.s.settimeout(0.5)
            try:
                self.buf += self.s.recv(65536)
            except socket.timeout:
                continue
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                m = json.loads(line) if line.strip() else {}
                if m.get("request_id") == rid:
                    return m.get("data")
        raise SystemExit(f"no answer to {args}")

    def get(self, name):
        return self.cmd("get_property", name)

    def wait(self, fn, timeout=5.0):
        until = time.time() + timeout
        while time.time() < until:
            v = fn()
            if v:
                return v
            time.sleep(0.01)
        return None

    def load(self, path):
        self.cmd("loadfile", path)
        return self.wait(lambda: self.get("path") == path and self.get("duration"), 10)

    def quit(self):
        try:
            self.s.sendall(b'{"command":["quit"]}\n')
            self.p.wait(10)
        except (OSError, subprocess.TimeoutExpired):
            self.p.kill()


def plumbing(a, work):
    plugins = os.path.join(work, "plugins")
    os.makedirs(os.path.join(plugins, "scrubprobe"))
    os.makedirs(os.path.join(plugins, "gyroflow"))
    with open(os.path.join(plugins, "scrubprobe", "main.lua"), "w") as f:
        f.write(PROBE)
    shutil.copy(a.helper, os.path.join(plugins, "gyroflow", "afp-scrub"))
    log = open(os.path.join(work, "mpv.log"), "w")
    m = Mpv(a.mpv, os.path.join(work, "mpv.sock"), log, ["--script=" + os.path.join(plugins, "scrubprobe")])
    try:
        check("the helper found", (m.wait(lambda: m.get("user-data/probe")) or {}).get("available"), True)
        d = m.load(a.clip)
        shown = lambda: m.get("user-data/afp-scrub/shown")

        def newer(than):
            return m.wait(lambda: (lambda v: v if v and v["id"] > than else None)(shown()), 3)

        t1 = d * 0.4
        # The mouse over the bar first (the helper started), then over the track
        m.cmd("script-message-to", "scrubprobe", "warm")
        time.sleep(0.3)
        m.cmd("script-message-to", "scrubprobe", "want", str(t1), "800")
        s = newer(0)
        req = (m.get("user-data/afp-scrub/request") or "").split("\t")
        check("asked: the sidecar's exact frame", len(req) >= 8 and req[1].lower().endswith(".lrf") and req[3] == "exact" and req[7] == a.clip)
        check("on screen", bool(s))
        if not s:
            return m
        print(f"     first picture {s['ms']:.0f} ms (the helper {s['helper_ms']:.0f}, {s['kind']}), {s['w']}x{s['h']} at {s['x']},{s['y']}")
        check("the frame at the mouse's time (within a sidecar frame)", abs(s["t"] - t1) < 1 / 29.97)
        check("above the bar, centred on the mouse", s["y"] + s["h"] < H - 66 and abs(s["x"] + s["w"] / 2 - 800) < 2)
        # Along the bar: 30 moves, a pixel's time apart (the bar ~1570 px for the clip)
        step = d / (W - 28)
        ms, kinds, last = [], {}, s["id"]
        for i in range(1, 31):
            m.cmd("script-message-to", "scrubprobe", "want", str(t1 + i * 3 * step), str(800 + 3 * i))
            n = newer(last)
            if n:
                last = n["id"]
                ms.append(n["ms"])
                kinds[n["kind"]] = kinds.get(n["kind"], 0) + 1
        ms.sort()
        check("along the bar: a picture each move (or the same frame)", len(ms) >= 20)
        if ms:
            print(f"     along the bar: median {ms[len(ms) // 2]:.1f} ms, p90 {ms[int(len(ms) * .9)]:.1f}, max {ms[-1]:.1f} "
                  f"(asked -> on screen), {kinds}")
        check("along the bar: no seek (walk or kept)", kinds.get("exact", 0) <= 1)
        # Back over the same stretch: kept in memory
        m.cmd("script-message-to", "scrubprobe", "want", str(t1), "800")
        n = newer(last)
        check("again: from memory", bool(n) and n["kind"], "cache")
        last = n["id"] if n else last
        # Gone when the mouse leaves; asked just before, its answer isn't shown
        m.cmd("script-message-to", "scrubprobe", "want", str(d * 0.9), "1400")
        m.cmd("script-message-to", "scrubprobe", "hide")
        time.sleep(0.3)
        check("left: gone, a late answer not shown", shown(), None)
        # A moment's link: the original's sidecar at the original's time
        m.cmd("set_property", "user-data/workspace", {"path": a.clip, "clip": a.clip, "offset": 5})
        m.cmd("script-message-to", "scrubprobe", "forget")
        m.cmd("script-message-to", "scrubprobe", "want", "1", "300")
        time.sleep(0.2)
        req = (m.get("user-data/afp-scrub/request") or "").split("\t")
        check("a moment: at the original's time", len(req) >= 3 and abs(float(req[2]) - 6) < 1e-3)
        m.cmd("script-message-to", "scrubprobe", "hide")
        if a.clip2:
            d2 = m.load(a.clip2)
            m.cmd("set_property", "user-data/workspace", {})
            m.cmd("script-message-to", "scrubprobe", "want", str(d2 * 0.5), "800")
            s = m.wait(shown, 5)
            req = (m.get("user-data/afp-scrub/request") or "").split("\t")
            check("no sidecar: the clip's keyframe", len(req) >= 4 and req[1] == a.clip2 and req[3] == "key")
            check("no sidecar: on screen, at or before the time", bool(s) and s["t"] <= d2 * 0.5 + 1e-3)
            if s:
                print(f"     keyframe picture {s['ms']:.0f} ms ({s['kind']}), frame at {s['t']:.3f} for {d2 * 0.5:.3f}")
    finally:
        m.quit()
        log.close()
    with open(os.path.join(work, "mpv.log")) as f:
        bad = [l.strip() for l in f if "afp-scrub ended" in l or "overlay" in l.lower() and "error" in l.lower()]
    check("no helper or overlay errors in mpv's log", bad, [])


def frame_with(mpv, clip, t, ass, out):
    """The clip's frame at t, W x H, with the .ass over it (as subtitles: vo=image draws those, not the OSD)"""
    tmp = tempfile.mkdtemp()
    subprocess.run([mpv, "--no-config", "--ao=null", "--vo=image", f"--vo-image-outdir={tmp}", "--vo-image-format=png",
                    f"--start={t:.3f}", "--frames=1", f"--vf=scale=w={W}:h={H}", f"--sub-file={ass}", "--sub-ass-override=no",
                    "--msg-level=all=error", clip], check=True)
    shutil.move(os.path.join(tmp, sorted(os.listdir(tmp))[-1]), out)


def shots(a, work):
    """The bar hovered, as the panel draws it (its dump), over the clip's frame, the helper's picture placed in it"""
    os.makedirs(a.shots, exist_ok=True)
    # (a folder named side-panel: mpv names the script after it)
    before_panel = os.path.join(work, "before-src", "side-panel")
    os.makedirs(before_panel)
    files = subprocess.run(["git", "-C", REPO, "ls-tree", "--name-only", a.before, "mpv/scripts/side-panel/"],
                           capture_output=True, text=True, check=True).stdout.split()
    for f in files:
        with open(os.path.join(before_panel, os.path.basename(f)), "wb") as out:
            out.write(subprocess.run(["git", "-C", REPO, "show", f"{a.before}:{f}"], capture_output=True, check=True).stdout)
    for name, panel, clip, preview in (("before.png", before_panel, a.clip, False), ("after.png", PANEL, a.clip, True),
                                       ("after-keyframe.png", PANEL, a.clip2, True)):
        if not clip:
            continue
        sub = os.path.join(work, name.replace(".png", ""))
        os.makedirs(sub)
        log = open(os.path.join(sub, "mpv.log"), "w")
        m = Mpv(a.mpv, os.path.join(sub, "s"), log, ["--script=" + panel])
        try:
            d = m.load(clip)
            pos, hover_t = d * 0.3, d * 0.62
            m.cmd("seek", str(pos), "absolute+exact")
            time.sleep(0.5)
            mx = 14 + (W - 28) * hover_t / d
            ass = os.path.join(sub, "bar.ass")
            m.cmd("script-message-to", "side_panel", "dump", ass, str(W), str(H), "1", f"{mx:.1f}", str(H - 66 + 21), "bar:track",
                  "232x130" if preview else "")
            m.wait(lambda: os.path.exists(ass), 5)
        finally:
            m.quit()
            log.close()
        out = os.path.join(a.shots, name)
        frame_with(a.mpv, clip, pos, ass, out)
        rect = [l.split() for l in open(ass) if l.startswith("; preview")]
        if preview and rect:
            x, y, w, h, t = int(rect[0][2]), int(rect[0][3]), int(rect[0][4]), int(rect[0][5]), float(rect[0][6])
            sidecar = None
            for n in (os.path.splitext(clip)[0] + e for e in (".LRF", ".lrf")):
                if os.path.exists(n):
                    sidecar = n
            bgra = os.path.join(sub, "pic.bgra")
            line = "\t".join(["1", sidecar or clip, f"{t:.3f}", "exact" if sidecar else "key", bgra, str(w), str(h)] + ([clip] if sidecar else []))
            r = subprocess.run([a.helper], input=line + "\n", capture_output=True, text=True).stdout
            check(f"{name}: the helper's picture", r.split("\t")[1:2], ["ok"])
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", out, "-f", "rawvideo", "-pix_fmt", "bgra", "-s", f"{w}x{h}", "-i", bgra,
                            "-filter_complex", f"[0][1]overlay={x}:{y}", out + ".tmp.png"], check=True)
            os.replace(out + ".tmp.png", out)
        print(f"     {out}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mpv", required=True)
    ap.add_argument("--helper", required=True)
    ap.add_argument("--clip", required=True)
    ap.add_argument("--clip2")
    ap.add_argument("--shots")
    ap.add_argument("--before", default="origin/main")
    a = ap.parse_args()
    a.clip = os.path.abspath(a.clip)
    a.clip2 = a.clip2 and os.path.abspath(a.clip2)
    work = tempfile.mkdtemp(prefix="scrub-ipc-")
    plumbing(a, work)
    if a.shots:
        shots(a, work)
    print(f"{failures} failed (work folder {work})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
