#!/usr/bin/env python3
"""Pipeline builds in the real player, start by start (docs/design/gpu-render.md, "Pipeline builds"). Opens a window.

  tools/pipebench/live_starts.py <ActionFootagePlayer.app> <out dir> <clip> [<clip> ...]

Each clip is opened in a player process of its own, as a double click in the Library does: the app's mpv with the
app's arguments (macos/player.sh) and the shader cache everyone uses (~/Library/Caches/io.mpv), but the plugins' data,
mpv's config folder and the journal copied to <out dir> (nothing of the user's is written). It plays 8 s, turns the
underwater colour over (as the side panel's button does), 6 s, turns it back, 6 s, and quits. A clip listed twice is
opened twice (the second start finds what the first left in the caches).

Per start it prints: the time to the first frame and to the first stabilized one, the pipelines libplacebo built (SPIR-V
compiles, pipelines, ms) at the start and after each turn, and how long each turn took to the last build. The logs stay
in <out dir> (libplacebo at debug: every new pass's source, for diffs between starts), with MoltenVK's own timing
(MVK_CONFIG_PERFORMANCE_TRACKING) in <out dir>/run-N.stderr.
"""
import json, os, re, shutil, socket, subprocess, sys, time

PLUGINS = ["workspace", "gyroflow", "view360", "dlogm", "dji-telemetry", "highlights", "footage-menu", "side-panel",
           "sdr-check", "agent", "any-layout", "live-photo"]


def env_for(app, out):
    c = app + "/Contents"
    res = c + "/Resources"
    data = os.path.expanduser("~/Library/Application Support/ActionFootagePlayer")
    mine = out + "/data"
    if not os.path.isdir(mine):
        os.makedirs(mine)
        for d in ["dlogm", "gyroflow", "mpv"]:
            if os.path.isdir(data + "/" + d):
                shutil.copytree(data + "/" + d, mine + "/" + d)
        for f in ["curated_lenses.json"]:
            if os.path.exists(data + "/" + f):
                shutil.copy(data + "/" + f, mine)
        os.makedirs(mine + "/agent", exist_ok=True)
    env = dict(os.environ)
    env.update(DLOGM_DATA=mine + "/dlogm", GYROFLOW_DATA=mine + "/gyroflow", AGENT_DATA=mine + "/agent",
               AFP_JOURNAL_DIR=out + "/journal", MPVBUNDLE="true")
    env["PATH"] = c + "/Helpers:" + env["PATH"]
    homes = sorted(d for d in os.listdir(c + "/Frameworks/Python.framework/Versions") if d[0].isdigit()) \
        if os.path.isdir(c + "/Frameworks/Python.framework/Versions") else []
    if homes:
        home = c + "/Frameworks/Python.framework/Versions/" + homes[-1]
        env["PYTHONHOME"] = home
    icd = res + "/vulkan/icd.d/MoltenVK_icd.json"
    env.update(VK_DRIVER_FILES=icd, VK_ICD_FILENAMES=icd, GYROFLOW_TOOLS=c + "/Helpers")
    # MoltenVK's own account of where pipeline time goes, logged when the device is destroyed
    env.update(MVK_CONFIG_PERFORMANCE_TRACKING="1", MVK_CONFIG_ACTIVITY_PERFORMANCE_LOGGING_STYLE="2",
               MVK_CONFIG_LOG_LEVEL="3")
    return env, mine


class Ipc:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX)
        for _ in range(200):
            try:
                self.s.connect(path)
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise SystemExit("no IPC socket " + path)
        self.s.settimeout(0.2)
        self.buf = b""
        self.events = []

    def send(self, *cmd):
        self.s.sendall(json.dumps({"command": list(cmd)}).encode() + b"\n")

    def pump(self, seconds):
        end = time.time() + seconds
        while time.time() < end:
            try:
                d = self.s.recv(65536)
                if not d:
                    return
                self.buf += d
            except socket.timeout:
                continue
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                try:
                    m = json.loads(line)
                except ValueError:
                    continue
                if m.get("event"):
                    self.events.append((time.time(), m))


SPENT = re.compile(r"^\[\s*([\d.]+)\].*Spent ([\d.]+) ms (translating SPIR-V|creating pipeline|re-specializing shader)")
TIME = re.compile(r"^\[\s*([\d.]+)\]")


def builds(log, t0, t1):
    n = {"translating SPIR-V": [0, 0.0], "creating pipeline": [0, 0.0], "re-specializing shader": [0, 0.0]}
    last = None
    for line in open(log, errors="replace"):
        m = SPENT.match(line)
        if m and t0 <= float(m.group(1)) < t1:
            n[m.group(3)][0] += 1
            n[m.group(3)][1] += float(m.group(2))
            last = float(m.group(1))
    return n, last


def first_time(log, pattern):
    r = re.compile(pattern)
    for line in open(log, errors="replace"):
        if r.search(line):
            m = TIME.match(line)
            if m:
                return float(m.group(1))
    return None


def main():
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    app, out, clips = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2]), sys.argv[3:]
    os.makedirs(out, exist_ok=True)
    env, mine = env_for(app, out)
    res = app + "/Contents/Resources"
    version = open(res + "/VERSION").read().strip() if os.path.exists(res + "/VERSION") else "?"
    print("app", version, "| shader cache", len(os.listdir(os.path.expanduser("~/Library/Caches/io.mpv"))), "files")
    for i, clip in enumerate(clips, 1):
        sock = "/tmp/afp-pb-%d-%d.sock" % (os.getpid(), i)
        log = "%s/run-%d.log" % (out, i)
        args = [app + "/Contents/Helpers/mpv", "--config-dir=" + mine + "/mpv", "--load-scripts=no",
                "--include=" + res + "/app.conf", "--script-opts-append=footage_menu-version=" + version]
        args += ["--script=%s/plugins/%s" % (res, p) for p in PLUGINS if os.path.isdir("%s/plugins/%s" % (res, p))]
        args += ["--msg-level=all=v,cplayer=debug,vo/gpu-next/libplacebo=debug", "--msg-time", "--log-file=" + log,
                 "--input-ipc-server=" + sock, "--force-window=immediate", os.path.abspath(clip)]
        with open("%s/run-%d.stderr" % (out, i), "w") as err:
            p = subprocess.Popen(args, env=env, stdout=err, stderr=err, stdin=subprocess.DEVNULL)
            ipc = Ipc(sock)
            ipc.send("observe_property", 1, "playback-time")
            ipc.pump(8)
            for _ in range(2):
                ipc.send("script-message-to", "dlogm", "uw-set", "uw", "toggle")
                ipc.pump(6)
            ipc.send("quit")
            try:
                p.wait(20)
            except subprocess.TimeoutExpired:
                p.kill()
        # The log's clock starts with mpv; the launch is within ~0.1 s of it
        playing = first_time(log, r"\] Playing: ")
        restart = first_time(log, r"playback-restart|Starting playback")
        warp = first_time(log, r"stabilizing on the GPU")
        ts = []
        for line in open(log, errors="replace"):
            if re.search(r'args="uw-set", args="uw", args="toggle"', line):
                ts.append(float(TIME.match(line).group(1)))
        ts = (ts + [None, None])[:2]
        end = 1e9
        start_b, start_last = builds(log, 0, ts[0] or end)
        # Each turn's builds: up to the next turn, the last one's 6 s (the quit's own come after)
        print("\nrun %d: %s" % (i, os.path.basename(clip)))
        fmt = lambda b: "SPIR-V %d (%.0f ms), pipelines %d (%.0f ms), respec %d" % (
            b["translating SPIR-V"][0], b["translating SPIR-V"][1], b["creating pipeline"][0], b["creating pipeline"][1],
            b["re-specializing shader"][0])
        print("  start: playing at %s s, playback started %s s, warp set %s s, last build %s s" % (
            playing, restart, warp, start_last))
        print("  start builds:", fmt(start_b))
        for name, t, nxt in [("uw on", ts[0], ts[1]), ("uw off", ts[1], None)]:
            if t is None:
                continue
            b, last = builds(log, t, nxt or t + 6)
            print("  %s at %.2f s: %s; last build %s" % (name, t, fmt(b), "%.2f s after" % (last - t) if last else "none"))


if __name__ == "__main__":
    main()
