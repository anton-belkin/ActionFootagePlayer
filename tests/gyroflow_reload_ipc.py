#!/usr/bin/env python3
"""The open player takes up a Gyroflow project saved while the clip plays (Ctrl+e, main.lua's check_project and
reload_project), headless (macOS, the app's own mpv, no window), for

  edit    an existing project saved once in Gyroflow
  twice   saved, then twice again within one second at the same size (mpv's mtimes are whole seconds)
  appear  a clip without motion data: the project Gyroflow saves next to it appears

Each case passes when the new shader (the glsl-shaders list's warp) is in place within a few seconds of the save, the
clip playing on from where it was, and the warp is the one a fresh open of the saved project gets.

  tests/gyroflow_reload_ipc.py [--lua <dir>] [--table <gyroflow-gpu-table>] [--cases edit,twice,appear] [--keep] <ActionFootagePlayer.app> <clip with motion
                                data, DJI .MP4>

The app is cloned into a temporary folder (cp -c) with this checkout's gyroflow plugin Lua put in it (or --lua's
folder's: another branch's mpv/scripts/gyroflow), and two changes
for the test alone: Ctrl+e's `open -b xyz.gyroflow` is /usr/bin/true (no Gyroflow window), and the stabilization is
allowed with vo=null (mpv parses the warp but draws nothing: the picture isn't compared). The clip is cloned next to it; the
project made with the app's gyroflow-project; saves are made by editing its smoothness and FOV in place, as Gyroflow
writes it. The app's table server (gyroflow-gpu-table) is the app's own unless --table <binary> gives another.
"""
import glob, hashlib, json, os, shutil, socket, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)

PLUGINS = ["workspace", "gyroflow", "view360", "dlogm", "dji-telemetry", "highlights", "footage-menu", "side-panel", "sdr-check", "agent", "any-layout"]


def make_rig(app, clip, table, lua):
    rig = os.path.realpath(tempfile.mkdtemp(prefix="afp-reload-"))
    sock = os.path.join("/tmp", "afpgf-%d.sock" % os.getpid())  # short: macOS caps socket paths at 104
    subprocess.run(["cp", "-cR", app, rig + "/app.app"], check=True)
    g = rig + "/app.app/Contents/Resources/plugins/gyroflow"
    for f in glob.glob(os.path.join(lua, "*.lua")):
        shutil.copy(f, g)
    # The lists the plugin reads (the workspace plugin's registry, beside it in a checkout), where they are
    for f in glob.glob(os.path.join(lua, "..", "workspace", "registry.*")):
        shutil.copy(f, rig + "/app.app/Contents/Resources/plugins/workspace")
    if table:
        shutil.copy(table, g + "/gyroflow-gpu-table")
    main = open(g + "/main.lua").read()
    for a, b in [('"/usr/bin/open"', '"/usr/bin/true"'),
                 ('if vo ~= "" and not vo:find("gpu-next", 1, true) then',
                  'if vo ~= "" and vo ~= "null" and not vo:find("gpu-next", 1, true) then')]:
        if a not in main:
            sys.exit("main.lua changed: the test's patch doesn't apply: " + a)
        main = main.replace(a, b)
    open(g + "/main.lua", "w").write(main)
    os.makedirs(rig + "/clips")
    subprocess.run(["cp", "-c", clip, rig + "/clips/a.MP4"], check=True)
    # Without motion data: the video alone, its first 12 s
    subprocess.run([rig + "/app.app/Contents/Helpers/ffmpeg", "-v", "error", "-i", clip, "-map", "0:v:0", "-t", "12", "-c", "copy",
                    rig + "/clips/c.MP4"], check=True)
    return rig, sock


def env_for(rig):
    c = rig + "/app.app/Contents"
    res = c + "/Resources"
    env = dict(os.environ)
    for k, v in {"DLOGM_DATA": "data/dlogm", "GYROFLOW_DATA": "data/gyroflow", "AGENT_DATA": "data/agent",
                 "AFP_JOURNAL_DIR": "journal"}.items():
        env[k] = os.path.join(rig, v)
        os.makedirs(env[k], exist_ok=True)
    env["PATH"] = c + "/Helpers:" + env["PATH"]
    icd = res + "/vulkan/icd.d/MoltenVK_icd.json"
    env.update(VK_DRIVER_FILES=icd, VK_ICD_FILENAMES=icd, GYROFLOW_TOOLS=c + "/Helpers")
    return env


class Player:
    def __init__(self, rig, sock, clip, stabilized=True):
        self.rig, self.sock = rig, sock
        os.makedirs(rig + "/cfg/script-opts", exist_ok=True)
        open(rig + "/cfg/script-opts/gyroflow.conf", "w").write("")
        for f in glob.glob(rig + "/journal/*"):
            os.remove(f)
        res = rig + "/app.app/Contents/Resources"
        args = [rig + "/app.app/Contents/Helpers/mpv", "--config-dir=" + rig + "/cfg", "--load-scripts=no", "--include=" + res + "/app.conf",
                "--vo=null", "--ao=null", "--force-window=no", "--keep-open=yes", "--input-ipc-server=" + sock, "--msg-level=all=v", "--msg-time",
                "--log-file=" + rig + "/mpv.log"]
        args += ["--script=" + res + "/plugins/" + p for p in PLUGINS] + [clip]
        self.p = subprocess.Popen(args, env=env_for(rig), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                self.cmd("get_property", "pid")
                break
            except OSError:
                time.sleep(0.1)
        # Until stabilized (or 30 s; a clip without motion data plays as it is), then a second's play
        for _ in range(300 if stabilized else 0):
            s = self.state()
            if s["warp"]:
                break
            time.sleep(0.1)
        time.sleep(1)

    def cmd(self, *args):
        s = socket.socket(socket.AF_UNIX)
        s.connect(self.sock)
        s.sendall((json.dumps({"command": list(args)}) + "\n").encode())
        s.settimeout(10)
        data = b""
        while True:
            data += s.recv(65536)
            for line in data.decode(errors="replace").splitlines():
                m = json.loads(line)
                if "error" in m:
                    s.close()
                    return m

    def prop(self, p):
        return self.cmd("get_property", p).get("data")

    def state(self):
        warp = [x for x in (self.prop("glsl-shaders") or []) if "mpv-gyroflow-" in x]
        h = hashlib.md5(open(warp[-1], "rb").read()).hexdigest()[:10] if warp and os.path.exists(warp[-1]) else None
        return {"warp": h, "t": round(self.prop("time-pos") or -1, 2)}

    def wait_change(self, before, limit):
        t = time.time()
        while time.time() - t < limit:
            s = self.state()
            if s["warp"] != before["warp"]:
                return s, round(time.time() - t, 2)
            time.sleep(0.1)
        return self.state(), None

    def quit(self):
        self.cmd("quit")
        self.p.wait(timeout=30)
        steps = []
        for j in glob.glob(self.rig + "/journal/player-*.jsonl"):
            for line in open(j):
                e = json.loads(line)
                n = e.get("n", "")
                if n in ("warp-set", "gyroflow/edit") or "gpu-table" in n:
                    steps.append(n + ("(reused)" if e.get("reused") else ""))
        return steps


def save(project, smooth):
    d = json.load(open(project))
    for p in d["stabilization"]["smoothing_params"]:
        if p["name"] == "smoothness":
            p["value"] = smooth
    d["stabilization"]["fov"] = 1.0 + smooth  # the zoom: visible
    with open(project, "r+") as f:  # in place, as Gyroflow writes it
        f.seek(0)
        f.truncate()
        json.dump(d, f)


def run(rig, sock, case):
    g = rig + "/app.app/Contents/Resources/plugins/gyroflow"
    a = rig + "/clips/a.gyroflow"
    if os.path.exists(a):
        os.remove(a)
    subprocess.run([g + "/gyroflow-project", "--preset", g + "/default.gyroflow", rig + "/clips/a.MP4", a], check=True, capture_output=True)
    clip = rig + "/clips/" + ("c.MP4" if case == "appear" else "a.MP4")
    project = clip[:-4] + ".gyroflow"
    if case == "appear" and os.path.exists(project):
        os.remove(project)
    pl = Player(rig, sock, clip, case != "appear")
    pl.cmd("keypress", "Ctrl+e")
    time.sleep(0.5)
    pl.cmd("set_property", "pause", False)  # Ctrl+e pauses; it plays on, as it does when Gyroflow is used
    time.sleep(1.5)
    before = pl.state()
    log = ["edit on  %s" % before]
    if case == "appear":
        shutil.copy(a, project + ".tmp")
        os.rename(project + ".tmp", project)
        s, dt = pl.wait_change(before, 20)
        log.append("appeared %s after %s s" % (s, dt))
        ok = dt is not None
    else:
        save(project, 0.2)
        s, dt = pl.wait_change(before, 8)
        log.append("saved    %s after %s s" % (s, dt))
        ok = dt is not None and s["t"] >= before["t"]
        if case == "twice":
            time.sleep(1)
            while time.time() % 1 > 0.3:
                time.sleep(0.02)
            save(project, 0.3)  # "1.3" and "1.4": the same size
            time.sleep(0.4)
            save(project, 0.4)
            s1 = s
            s, dt = pl.wait_change(s1, 8)
            time.sleep(3)  # the last save taken up too
            s = pl.state()
            log.append("twice    %s after %s s" % (s, dt))
            ok = ok and dt is not None and s["t"] >= s1["t"]
    log.append("journal: " + " ".join(pl.quit()))
    fresh = Player(rig, sock, clip)
    f = fresh.state()
    fresh.quit()
    log.append("fresh    %s" % f)
    ok = ok and f["warp"] == s["warp"]
    return ok, log


def main():
    args = sys.argv[1:]
    def opt(name, default):
        if name not in args:
            return default
        i = args.index(name)
        v = args[i + 1]
        del args[i:i + 2]
        return os.path.abspath(v)
    table = opt("--table", None)
    lua = opt("--lua", os.path.join(REPO, "mpv/scripts/gyroflow"))
    keep = "--keep" in args
    if keep:
        args.remove("--keep")
    cases = ("edit", "twice", "appear")
    if "--cases" in args:
        i = args.index("--cases")
        cases = args[i + 1].split(",")
        del args[i:i + 2]
    if len(args) < 2:
        sys.exit(__doc__)
    app, clip = args[0], args[1]
    rig, sock = make_rig(os.path.abspath(app), os.path.abspath(clip), table, lua)
    failed = 0
    try:
        for case in cases:
            try:
                ok, log = run(rig, sock, case)
            except Exception as e:  # mpv gone: its log is in the rig (--keep)
                ok, log = False, ["%s: %s" % (type(e).__name__, e)]
            failed += not ok
            print("%s %s" % ("OK  " if ok else "FAIL", case))
            for line in log:
                print("     " + line)
            sys.stdout.flush()
    finally:
        # The cloned app registered with Launch Services by nothing (never opened): the folder alone goes
        if keep:
            print("rig kept: " + rig)
        else:
            shutil.rmtree(rig, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
