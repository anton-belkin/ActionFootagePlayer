#!/usr/bin/env python3
"""A clip with motion measured from the picture beside it (<stem>.afp-motion, vs-gyroflow's afp_motion.rs) plays
stabilized without a Gyroflow project, headless (macOS, the app's own mpv, no window), on the GPU path and through
VapourSynth:

  motion    only the .afp-motion beside the clip: stabilized in the player's own mode (kind "default", optical),
            the horizon lock off and refused (no gravity in measured motion)
  project   a Gyroflow project beside it too: the project plays (it wins)
  measure   neither, then Ctrl+m: the motion measured in the player (gyroflow-optical), the clip played again with
            it, stabilized as "motion"
  none      neither: not stabilized (a clip without motion data: as recorded, or its lens alone corrected)

  tests/afp_motion_ipc.py [--keep] [--shots <dir>] <ActionFootagePlayer.app> <clip with a Gyroflow project of optical motion beside it>
                          [gpu|vs ...]

The app is cloned into a temporary folder (cp -c) with this checkout's mpv plugins and vs-gyroflow's release
binaries (vs-gyroflow/target/release) put in it, and the GPU path allowed with vo=null (mpv parses the warp but draws
nothing). The clip and its project are copied there, and the .afp-motion made from the project with the rig's own
gyroflow-optical --from-project. Nothing is written beside the original clip. --shots: the side panel's Source and
Transform tabs for "motion" and "none" (on the GPU path) as pictures in <dir>, for the pull request.
"""
import glob, json, os, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from gyroflow_reload_ipc import PLUGINS, Player  # noqa: E402

BINARIES = ["gyroflow-probe", "gyroflow-project", "gyroflow-gpu-table", "gyroflow-export", "gyroflow-optical", "libvs_gyroflow.dylib"]


def make_rig(app, clip):
    rig = os.path.realpath(tempfile.mkdtemp(prefix="afp-motion-"))  # VSScript knows itself by its real path
    sock = os.path.join("/tmp", "afpmo-%d.sock" % os.getpid())  # short: macOS caps socket paths at 104
    subprocess.run(["cp", "-cR", app, rig + "/app.app"], check=True)
    plugins = rig + "/app.app/Contents/Resources/plugins"
    for p in PLUGINS:
        src = os.path.join(REPO, "mpv/scripts", p)
        if os.path.isdir(src) and os.path.isdir(os.path.join(plugins, p)):
            for f in glob.glob(os.path.join(src, "*.lua")):
                shutil.copy(f, os.path.join(plugins, p))
    g = plugins + "/gyroflow"
    for b in BINARIES:
        shutil.copy(os.path.join(REPO, "vs-gyroflow/target/release", b), g)
    main = open(g + "/main.lua").read()
    for a, b in [('"/usr/bin/open"', '"/usr/bin/true"'),
                 ('if vo ~= "" and not vo:find("gpu-next", 1, true) then return false end',
                  'if vo ~= "" and vo ~= "null" and not vo:find("gpu-next", 1, true) then return false end'),
                 ('(vo == "" or vo:find("gpu-next", 1, true))', '(vo == "" or vo == "null" or vo:find("gpu-next", 1, true))')]:
        if a not in main:
            sys.exit("main.lua changed: the test's patch doesn't apply: " + a)
        main = main.replace(a, b)
    open(g + "/main.lua", "w").write(main)
    os.makedirs(rig + "/clips")
    shutil.copy(clip, rig + "/clips/a.MP4")
    shutil.copy(os.path.splitext(clip)[0] + ".gyroflow", rig + "/clips/a.gyroflow")
    out = subprocess.run([g + "/gyroflow-optical", "--from-project", rig + "/clips/a.MP4"], capture_output=True, text=True)
    print("from-project:", out.stdout.strip() or out.stderr.strip())
    if out.returncode != 0:
        sys.exit("the motion file couldn't be made")
    return rig, sock


def gyroflow_state(pl):
    get = lambda k: pl.prop("user-data/gyroflow/" + k)
    return {"kind": get("kind"), "optical": get("optical"), "horizon_lock": get("horizon_lock"), "motion": get("motion"),
            "underwater": get("underwater")}


SHOTS = None


def shots(pl, name):
    """The side panel's Source and Transform tabs as they show the clip, drawn over its frame: <SHOTS>/<case>-<tab>.png
    (the panel's own dump of what it draws, as subtitles in mpv's screenshot: vo=null draws nothing itself)"""
    pl.cmd("set_property", "pause", True)
    for tab in ["source", "transform"]:
        ass = os.path.join(SHOTS, "%s-%s.ass" % (name, tab))
        png = os.path.join(SHOTS, "%s-%s.png" % (name, tab))
        if os.path.exists(ass):
            os.remove(ass)  # waited for below: an old one would be taken for it
        pl.cmd("script-message-to", "side_panel", "tab", tab)
        time.sleep(0.7)
        pl.cmd("script-message-to", "side_panel", "dump", ass, "1920", "1080", "1")
        for _ in range(30):
            if os.path.exists(ass):
                break
            time.sleep(0.1)
        pl.cmd("sub-add", ass, "select")
        time.sleep(0.5)
        pl.cmd("screenshot-to-file", png, "subtitles")
        pl.cmd("sub-remove")
        print("  shot", png)


def case(rig, sock, name, path):
    clips = rig + "/clips"
    project, motion = clips + "/a.gyroflow", clips + "/a.afp-motion"
    # Each case's files beside the clip: the project and motion file moved aside when it has none
    if name == "measure" and os.path.exists(motion + ".measured"):
        os.remove(motion + ".measured")
    for f, keep in [(project, name == "project"), (motion, name in ("motion", "project"))]:
        aside = f + ".aside"
        if keep and os.path.exists(aside):
            os.rename(aside, f)
        if not keep and os.path.exists(f):
            os.rename(f, aside)
    pl = Player(rig, sock, clips + "/a.MP4", path, name not in ("none", "measure"))
    s, g = pl.state(), gyroflow_state(pl)
    log = ["%s/%s: %s %s" % (name, path, s, json.dumps(g))]
    stabilized = bool(s["warp"] or s["vf"])
    if name == "motion":
        ok = stabilized and g["kind"] == "default" and isinstance(g["optical"], dict) and g["horizon_lock"] == "no"
        pl.cmd("script-message-to", "gyroflow", "horizon-lock", "yes")
        time.sleep(1.5)
        g2 = gyroflow_state(pl)
        log.append("  horizon-lock yes -> %s" % g2["horizon_lock"])
        ok = ok and g2["horizon_lock"] == "no"
    elif name == "project":
        ok = stabilized and g["kind"] == "project"
    elif name == "measure":
        # Not stabilized before: as recorded, or its lens alone
        before = g["kind"] not in ("", "lens")
        t = time.time()
        pl.cmd("keypress", "Ctrl+m")
        while time.time() - t < 120:
            time.sleep(0.5)
            try:
                g = gyroflow_state(pl)
                s = pl.state()
            except OSError:
                continue
            if isinstance(g["optical"], dict) and (s["warp"] or s["vf"]):
                break
        log.append("  Ctrl+m -> %s %s after %.1f s" % (s, json.dumps(g), time.time() - t))
        ok = not before and isinstance(g["optical"], dict) and bool(s["warp"] or s["vf"]) and g["optical"].get("how") == "measured"
        # What it measured stays aside for the cases after (they bring the converted one back)
        if os.path.exists(motion):
            os.rename(motion, motion + ".measured")
    else:
        # Not stabilized: played as recorded, or with its lens alone corrected (kind "lens") where its lens is known
        ok = g["kind"] in ("", "lens") and not g["optical"]
    if SHOTS and name in ("motion", "none") and path == "gpu":
        shots(pl, name)
    steps = pl.quit()
    mlog = open(rig + "/mpv.log", errors="replace").read()
    measured = [l.split("] ", 1)[-1] for l in mlog.splitlines() if "measured from the picture" in l or "measured motion" in l]
    log += ["  " + m[:220] for m in measured[:3]]
    return ok, log


def main():
    args = sys.argv[1:]
    global SHOTS
    keep = "--keep" in args
    args = [a for a in args if a != "--keep"]
    if "--shots" in args:
        i = args.index("--shots")
        SHOTS = os.path.abspath(args[i + 1])
        os.makedirs(SHOTS, exist_ok=True)
        del args[i:i + 2]
    if len(args) < 2:
        sys.exit(__doc__)
    app, clip, paths = args[0], args[1], args[2:] or ["gpu", "vs"]
    rig, sock = make_rig(app, clip)
    failed = 0
    try:
        for path in paths:
            for name in ["motion", "project", "none", "measure"]:
                ok, log = case(rig, sock, name, path)
                print(("ok    " if ok else "FAIL  ") + "\n".join(log))
                failed += not ok
    finally:
        if keep:
            print("rig kept:", rig)
        else:
            shutil.rmtree(rig, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
