#!/usr/bin/env python3
"""Task 1 repro without a window: the app's mpv with --vo=null driven over IPC (the frames the VO gets after each
pause-time command), fed through a port of warp.glsl's dn_window()/dn_state ring (which frame the shader shows).
Prints, after each step: the seek bar's time (time-pos), the frame on screen, whether it's denoised (neighbours)."""
import json, os, socket, subprocess, sys, time

MPV = os.path.expanduser("~/Applications/ActionFootagePlayer.app/Contents/Helpers/mpv")
CLIP = sys.argv[1]
SOCK = os.path.join(os.environ.get("TMPDIR", "/tmp"), "dnrepro.sock")

# ---- warp.glsl's ring, ported (DN_RING slots, gdn_head, gdn_pts; dn_window; dn_state's store) ----
DN_RADIUS, DN_RING = 5, 11
class Ring:
    def __init__(s, fps, after=5, radius=5):
        s.fps, s.after, s.radius = fps, after, radius
        s.pts = [-1e9] * DN_RING; s.head = 0; s.ok = False
    def render(s, PTS):
        same = s.ok and abs(PTS - s.pts[s.head]) < 1e-4
        nw = 0 if not s.ok else (s.head if same else (s.head + 1) % DN_RING)
        back_slot = lambda k: (nw - k + 2 * DN_RING) % DN_RING
        pts = [PTS if k == 0 else (s.pts[back_slot(k)] if s.ok else -1e9) for k in range(2 * DN_RADIUS + 1)]
        back = 0
        for k in range(2 * DN_RADIUS):
            dt = pts[k] - pts[k + 1]
            if not (dt > 1e-4 and dt < 2.6 / s.fps): break
            back += 1
        kc = min(s.after, back)
        nb_fwd, nb_back = kc, min(s.radius, back - kc)
        shown = pts[kc]
        fresh = (not same) or s.after == 0
        if fresh:
            if not s.ok: s.pts = [-1e9] * DN_RING
            s.head = nw; s.pts[nw] = PTS; s.ok = True
        return shown, nb_back, nb_fwd

# ---- mpv over IPC ----
def start():
    if os.path.exists(SOCK): os.remove(SOCK)
    p = subprocess.Popen([MPV, "--no-config", "--vo=null", "--ao=null", "--pause", "--autocreate-playlist=no",
                          "--input-ipc-server=" + SOCK, "--hr-seek=yes", "--keep-open=yes", CLIP],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        if os.path.exists(SOCK): break
        time.sleep(0.05)
    s = socket.socket(socket.AF_UNIX); s.connect(SOCK); s.settimeout(5)
    return p, s
rid = 0
buf = b""
def cmd(s, *args):
    global rid, buf
    rid += 1
    s.sendall((json.dumps({"command": list(args), "request_id": rid}) + "\n").encode())
    while True:
        while b"\n" not in buf: buf += s.recv(65536)
        line, buf = buf.split(b"\n", 1)
        m = json.loads(line)
        if m.get("request_id") == rid: return m.get("data")
def settle(s):
    # until no seek is under way and time-pos stops moving
    last = None
    for _ in range(100):
        time.sleep(0.08)
        t = cmd(s, "get_property", "time-pos")
        if t == last and not cmd(s, "get_property", "seeking"): return t
        last = t
    return last

p, s = start()
settle(s)
fps = cmd(s, "get_property", "container-fps")
F = 1.0 / fps
ring = Ring(fps)
fr = lambda t: round(t * fps)   # frame number
rows = []
def note(step, vo_frames, bar):
    shown = None
    for t in vo_frames: shown, b, f = ring.render(t)
    if not vo_frames: shown, b, f = ring.render(bar)   # a redraw of the same frame
    rows.append((step, fr(bar), fr(shown), b, f))

# Play 2 s from 10 s: every frame reaches the VO in a row
cmd(s, "seek", "10", "absolute+exact"); t0 = settle(s)
note("seek 10 s (paused)", [t0], t0)
cmd(s, "set_property", "pause", False); time.sleep(2.0); cmd(s, "set_property", "pause", True); t1 = settle(s)
frames = [t0 + k * F for k in range(1, fr(t1) - fr(t0) + 1)]
note("play 2 s, pause", frames, t1)
for i in range(3):
    before = cmd(s, "get_property", "time-pos"); cmd(s, "frame-step"); t = settle(s)
    note("frame-step  .", [t], t)
for i in range(3):
    cmd(s, "frame-back-step"); t = settle(s)
    note("frame-back-step  ,", [t], t)
for i in range(6):
    cmd(s, "frame-step"); t = settle(s)
    note("frame-step  .", [t], t)
cmd(s, "seek", "20", "absolute+exact"); t = settle(s)
note("click on the bar: seek 20 s exact", [t], t)
for i in range(7):
    cmd(s, "frame-step"); t = settle(s)
    note("frame-step  .", [t], t)
cmd(s, "quit"); p.wait()

print(f"clip {os.path.basename(CLIP)}, {fps:.3f} fps; frames numbered from 0; Full denoise (5 after, 5 before)")
print(f"{'step':36} {'bar (time-pos)':>14} {'on screen':>10} {'diff':>5} {'neighbours':>11}")
for step, bar, shown, b, f in rows:
    print(f"{step:36} {bar:14d} {shown:10d} {shown - bar:5d} {b:5d}+{f:<5d}")
