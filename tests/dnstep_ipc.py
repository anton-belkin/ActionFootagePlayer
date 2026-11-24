#!/usr/bin/env python3
"""Frame steps and the paused frame with the GPU path's temporal denoise (docs/seek-and-scrub.md, problem 1), in the
app's own mpv with its plugins, headless (macOS, --vo=null: the shader is parsed, not drawn; which frame it shows is
tests/dnstep_test.lua's model). Over the JSON IPC, keys pressed as you would: after each, the frame on screen (the
gyroflow plugin's user-data/gyroflow/gpu_delay, what the seek bar shows), the frame it asks of the shader
(glsl-shader-opts gyroflow_dn_show) and the newest frame decoded (time-pos):

  - paused after playing: on screen 5 frames behind time-pos, asked for nothing;
  - `.`: one frame on each, time-pos one on;
  - `,`: one frame back each, from the ring (time-pos stays), asked of the shader;
  - stopped for a moment: the ring filled again around it (time-pos 5 after it, the frame the same);
  - a click on the bar (an exact seek) while paused: that frame on screen, then filled, the frame the same;
  - `,` seven times quickly: past the ring, a seek, then filled; the frame 7 back;
  - playing on: from the frame on screen.

  tests/dnstep_ipc.py [--keep] <ActionFootagePlayer.app built from this checkout> <clip with motion data, DJI .MP4>
"""
import json, os, shutil, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gyroflow_reload_ipc import make_rig, Player, REPO  # noqa: E402

failures = 0


def check(what, got, want):
    global failures
    ok = got == want
    if not ok:
        failures += 1
    print(("ok   " if ok else "FAIL ") + what + ("" if ok else f": got {got!r}, want {want!r}"))
    sys.stdout.flush()


class Rig:
    def __init__(self, pl):
        self.pl = pl
        self.fps = pl.prop("container-fps")
        self.F = 1 / self.fps

    def frame(self, t):
        return None if t is None or t < 0 else round(t * self.fps)

    def now(self):
        # time-pos, the frame on screen and the one asked of the shader, as frame numbers
        t = self.pl.prop("time-pos")
        d = self.pl.prop("user-data/gyroflow/gpu_delay") or {}
        show = d.get("show", -1)
        if show is not None and show >= 0:
            shown = show
        elif t is not None and d.get("delay", 0) > 0:
            restart = d.get("restart", -1)
            r = min(restart, t) if restart is not None and restart >= 0 else t
            shown = max(r, t - d["delay"])
        else:
            shown = t
        opts = self.pl.prop("glsl-shader-opts") or {}
        asked = float(opts.get("gyroflow_dn_show", "-1"))
        return {"t": self.frame(t), "shown": self.frame(shown), "asked": self.frame(asked) if asked >= 0 else None,
                "delay": round(d.get("delay", 0) * self.fps)}

    def settle(self, quiet=0.35, limit=8):
        # Until nothing changes for a while (seeks, frame-steps, the fill done) and it's paused
        last, since, t0 = None, time.time(), time.time()
        while time.time() - t0 < limit:
            s = (self.now(), self.pl.prop("pause"), self.pl.prop("seeking"))
            if s != last:
                last, since = s, time.time()
            elif time.time() - since >= quiet and s[1] and not s[2]:
                return s[0]
            time.sleep(0.03)
        return last[0]

    def key(self, k):
        self.pl.cmd("keypress", k)


def main():
    args = sys.argv[1:]
    keep = "--keep" in args
    if keep:
        args.remove("--keep")
    if len(args) != 2:
        sys.exit(__doc__)
    app, clip = os.path.abspath(args[0]), os.path.abspath(args[1])
    rig, sock = make_rig(app, clip, None, os.path.join(REPO, "mpv/scripts/gyroflow"))
    pl = None
    try:
        pl = Player(rig, sock, rig + "/clips/a.MP4", "gpu")
        r = Rig(pl)
        print(f"clip at {r.fps:.3f} fps, warp {pl.state()['warp']}")
        pl.cmd("set_property", "pause", True)
        pl.cmd("seek", "10", "absolute+exact")
        r.settle()
        pl.cmd("set_property", "pause", False)
        time.sleep(1.5)
        pl.cmd("set_property", "pause", True)
        # Paused while playing: its frame has its neighbours, nothing to fill
        s = r.settle(quiet=0.8)
        check("the denoise delays the picture 5 frames", s["delay"], 5)
        check("paused: on screen 5 behind time-pos", s["t"] - s["shown"], 5)
        check("paused: nothing asked of the shader", s["asked"], None)
        start = s["shown"]
        for i in range(1, 4):
            r.key(".")
            s = r.settle()
            check(f". {i}: on screen", s["shown"], start + i)
            check(f". {i}: time-pos", s["t"], start + i + 5)
        newest = s["t"]
        for i in range(1, 4):
            r.key(",")
            time.sleep(0.05)
            s = r.now()
            check(f", {i}: on screen at once", s["shown"], start + 3 - i)
            check(f", {i}: asked of the shader", s["asked"], start + 3 - i)
            check(f", {i}: from the ring (time-pos stays)", s["t"], newest)
        # Stopped: filled again around it (still_delay 0.4 s, then a seek and 10 frames)
        s = r.settle(quiet=1.0)
        check("filled: the frame the same", s["shown"], start)
        check("filled: time-pos 5 after it", s["t"], start + 5)
        # A click on the bar while paused
        pl.cmd("seek", "20", "absolute+exact")
        s = r.settle(quiet=1.0)
        target = r.frame(20.0)
        check("click: on screen", abs(s["shown"] - target) <= 1, True)
        clicked = s["shown"]
        check("click, filled: time-pos 5 after it", s["t"], clicked + 5)
        r.key(".")
        s = r.settle()
        check("click, .: one frame on", s["shown"], clicked + 1)
        # Seven steps back quickly: past the ring
        for _ in range(7):
            r.key(",")
            time.sleep(0.02)
        s = r.settle(quiet=1.0)
        check("7 ,: on screen", s["shown"], clicked + 1 - 7)
        check("7 ,: filled, time-pos 5 after it", s["t"], clicked + 1 - 7 + 5)
        # Two back from the ring, then playing on: from the frame on screen
        r.key(",")
        r.key(",")
        s = r.settle(quiet=0.2)
        here = s["shown"]
        pl.cmd("set_property", "pause", False)
        time.sleep(0.6)
        pl.cmd("set_property", "pause", True)
        s = r.settle(quiet=1.0)
        played = s["shown"] - here
        check(f"played on from the frame on screen ({played} frames in ~0.6 s)", 0 < played < 0.9 * r.fps, True)
        check("played on: nothing asked of the shader", s["asked"], None)
    finally:
        if pl:
            try:
                pl.cmd("quit")
                pl.p.wait(timeout=30)
            except Exception:
                pass
        if keep:
            print("rig kept: " + rig)
        else:
            shutil.rmtree(rig, ignore_errors=True)
    print(f"{failures} failed")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
