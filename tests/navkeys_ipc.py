#!/usr/bin/env python3
"""The arrows with a modifier in a running mpv with the side panel (mpv/scripts/side-panel/navkeys.lua), keys pressed
over its JSON IPC (no window: --vo=null):

  - Option (Alt)+→ / ← steps a frame on / back, paused; Cmd+→ / ← (Ctrl elsewhere) the next / previous clip;
  - plain ← / → still seek 5 s;
  - the keys are what the bar's buttons name (input-bindings: the command's winning key);
  - a key bound in your input.conf is left as you have it.

    python3 tests/navkeys_ipc.py --mpv <mpv> [--work <scratch dir>] [--platform-mac | --platform-other]

Needs ffmpeg. The platform is the running mpv's (Cmd on a Mac, Ctrl elsewhere).
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


class Mpv:
    def __init__(self, mpv, sock, log, work, extra=()):
        self.p = subprocess.Popen([mpv, "--no-config", "--idle=yes", "--vo=null", "--ao=null", f"--input-ipc-server={sock}",
                                   "--pause", "--hr-seek=yes", "--script=" + os.path.join(REPO, "mpv/scripts/side-panel"), *extra],
                                  stdout=log, stderr=subprocess.STDOUT, cwd=work)
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

    def get(self, prop):
        return self.cmd("get_property", prop)

    def settle(self, cond, timeout=8):
        until = time.time() + timeout
        while time.time() < until:
            if cond():
                return True
            time.sleep(0.05)
        return False

    def quit(self):
        try:
            self.cmd("quit")
        except SystemExit:
            pass
        self.p.wait(10)


def winner(bindings, cmd):
    best = None
    for b in bindings:
        if b.get("cmd") == cmd and not b["key"].startswith(("MBTN", "MOUSE", "WHEEL")):
            if best is None or b["priority"] > best["priority"]:
                best = b
    return best and best["key"]


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--mpv", required=True)
    a.add_argument("--work")
    o = a.parse_args()
    work = o.work or tempfile.mkdtemp(prefix="afp-navkeys-")
    if os.path.exists(work):
        shutil.rmtree(work)
    os.makedirs(work)
    clips = [os.path.join(work, n) for n in ("A.mp4", "B.mp4", "C.mp4")]
    for c in clips:
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=25:duration=20",
                        "-c:v", "libx264", "-g", "250", "-pix_fmt", "yuv420p", c], check=True)
    sock = f"/tmp/afp-nk-{os.getpid()}.sock"
    log = open(os.path.join(work, "mpv.log"), "w")
    m = Mpv(o.mpv, sock, log, work)
    try:
        m.settle(lambda: winner(m.get("input-bindings"), "frame-step") == "Alt+RIGHT")
        mac = m.get("platform") == "darwin"
        clip = "Meta+" if mac else "Ctrl+"
        for c in clips:
            m.cmd("loadfile", c, "append-play")
        m.cmd("playlist-play-index", "1")
        check("B plays", m.settle(lambda: m.get("path") == clips[1] and m.get("time-pos") is not None), True)
        m.cmd("seek", "10", "absolute+exact")
        m.settle(lambda: abs((m.get("time-pos") or 0) - 10) < 0.01 and m.get("seeking") is False)
        time.sleep(0.5)  # (the seek done: a frame step meanwhile is dropped)
        check("at 10 s", round(m.get("time-pos"), 3), 10.0)

        # A frame on and back: 1/25 s, paused
        m.cmd("keypress", "Alt+RIGHT")
        check("Option+→: a frame on", m.settle(lambda: abs((m.get("time-pos") or 0) - 10.04) < 0.005), True)
        check("…paused", m.get("pause"), True)
        m.cmd("keypress", "Alt+LEFT")
        check("Option+←: a frame back", m.settle(lambda: abs((m.get("time-pos") or 0) - 10) < 0.005), True)
        time.sleep(0.3)  # (a step back is a seek: one pressed meanwhile is dropped, as with mpv's own ",")
        m.cmd("keypress", "Alt+LEFT")
        check("…and another", m.settle(lambda: abs((m.get("time-pos") or 0) - 9.96) < 0.005), True)
        # Plain arrows: 5 s, as mpv's own
        m.cmd("keypress", "RIGHT")
        check("→ still seeks 5 s", m.settle(lambda: abs((m.get("time-pos") or 0) - 14.96) < 0.6), True)
        # The playlist
        m.cmd("keypress", clip + "RIGHT")
        check(clip + "→: the next clip", m.settle(lambda: m.get("path") == clips[2]), True)
        m.cmd("keypress", clip + "LEFT")
        m.cmd("keypress", clip + "LEFT")
        check(clip + "← twice: back to the first", m.settle(lambda: m.get("path") == clips[0]), True)

        # What the bar's buttons name: these keys win for the commands
        b = m.get("input-bindings")
        check("frame-step's key", winner(b, "frame-step"), "Alt+RIGHT")
        check("frame-back-step's key", winner(b, "frame-back-step"), "Alt+LEFT")
        check("playlist-next's key", winner(b, "playlist-next"), clip + "RIGHT")
        check("playlist-prev's key", winner(b, "playlist-prev"), clip + "LEFT")
    finally:
        m.quit()
        log.close()
        if os.path.exists(sock):
            os.unlink(sock)

    # Your own input.conf: its key for Cmd (Ctrl)+→ stays yours
    conf = os.path.join(work, "conf")
    os.makedirs(conf)
    with open(os.path.join(conf, "input.conf"), "w") as f:
        f.write(f"{clip}RIGHT seek 30\n")
    log = open(os.path.join(work, "mpv2.log"), "w")
    m = Mpv(o.mpv, sock, log, work, ["--config=yes", f"--config-dir={conf}"])
    try:
        # (the plugins load after the IPC is up)
        m.settle(lambda: winner(m.get("input-bindings"), "frame-step") == "Alt+RIGHT")
        b = m.get("input-bindings")
        mine = [x for x in b if x["key"] == clip + "RIGHT" and not x.get("is_weak")]
        check("your input.conf's key kept", [x["cmd"] for x in mine], ["seek 30"])
        check("the others bound", winner(b, "frame-step"), "Alt+RIGHT")
    finally:
        m.quit()
        log.close()
        if os.path.exists(sock):
            os.unlink(sock)
    print(f"{failures} failed; mpv's log: {os.path.join(work, 'mpv.log')}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
