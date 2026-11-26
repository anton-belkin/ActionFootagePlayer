#!/usr/bin/env python3
"""The as-recorded switch's latency (h, afp-compare; mpv/scripts/gyroflow/compare.lua), from the ask to the first frame
shown with it, in the app's own mpv with this checkout's plugins and look shaders. TAKES THE SCREEN: a player window
for ~30 s per clip. Ask Anton first (docs/dev/testing.md, "Rules for every live run").

  tools/compare_latency/run.py <ActionFootagePlayer.app> <out dir> <clip> [<clip> ...] [--cycles N] [--gap S]

The app is cloned into <out dir> (cp -c), this checkout's gyroflow and footage-menu Lua, the workspace registry and
the dlogm look shaders (baked again from shaders/*.glsl.in with the app's own LUTs) put into the clone. Per clip, mpv
runs with --macos-latency-log (patch 0010) and drive.lua, which switches as recorded on and off N times playing, then
N times paused. For each switch: the time from the ask (the host clock, in afp-compare's message) to the first frame
drawn with it (begin), and to that frame on screen (shown). The first switch of a run is listed apart: the first time
a frame is drawn without the warp may build pipelines the player hadn't needed yet.

Output: <out dir>/<clip>.log (mpv's log), <clip>-plays.png and <clip>-recorded.png (the window, paused, as it plays
and as recorded with its card), a table on stdout. Unregister the clone from Launch Services afterwards
(docs/dev/release.md).
"""
import glob, os, re, shutil, statistics, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
PLUGINS = ["workspace", "gyroflow", "view360", "dlogm", "dji-telemetry", "highlights", "footage-menu", "side-panel", "sdr-check", "agent", "any-layout"]


def make_clone(app, out):
    clone = os.path.join(out, "app.app")
    if os.path.exists(clone):
        shutil.rmtree(clone)
    subprocess.run(["cp", "-cR", app, clone], check=True)
    plugins = clone + "/Contents/Resources/plugins"
    for f in glob.glob(REPO + "/mpv/scripts/gyroflow/*.lua"):
        shutil.copy(f, plugins + "/gyroflow")
    shutil.copy(REPO + "/mpv/scripts/footage-menu/main.lua", plugins + "/footage-menu")
    for f in glob.glob(REPO + "/mpv/scripts/workspace/registry.*"):
        shutil.copy(f, plugins + "/workspace")
    sh = plugins + "/dlogm/shaders"
    for path in glob.glob(sh + "/*_hlg.glsl"):
        name = os.path.basename(path)
        if name == "underwater_hlg.glsl":
            shutil.copy(REPO + "/shaders/underwater_hlg.glsl", path)
            continue
        curve = name[:-len("_hlg.glsl")]
        template = REPO + ("/shaders/dlogm_hlg.glsl.in" if curve == "dlogm" else "/shaders/log_hlg.glsl.in")
        cube = plugins + "/dlogm/luts/" + curve + "_hlg.cube"
        if os.path.exists(cube):
            subprocess.run([sys.executable, "-I", REPO + "/tools/cube_to_glsl.py", template, cube, path], check=True, stdout=subprocess.DEVNULL)
    return clone


def run_clip(clone, out, clip, cycles, gap):
    rig = os.path.join(out, "rig")
    env = dict(os.environ)
    for k, v in {"DLOGM_DATA": "data/dlogm", "GYROFLOW_DATA": "data/gyroflow", "AGENT_DATA": "data/agent", "AFP_JOURNAL_DIR": "journal"}.items():
        env[k] = os.path.join(rig, v)
        os.makedirs(env[k], exist_ok=True)
    c = clone + "/Contents"
    icd = c + "/Resources/vulkan/icd.d/MoltenVK_icd.json"
    env.update(PATH=c + "/Helpers:" + env["PATH"], VK_DRIVER_FILES=icd, VK_ICD_FILENAMES=icd, GYROFLOW_TOOLS=c + "/Helpers")
    os.makedirs(rig + "/cfg/script-opts", exist_ok=True)
    log = os.path.join(out, os.path.basename(clip) + ".log")
    args = [c + "/Helpers/mpv", "--config-dir=" + rig + "/cfg", "--load-scripts=no", "--include=" + c + "/Resources/app.conf",
            "--autocreate-playlist=no", "--loop-file=inf", "--focus-on=never", "--geometry=1600x900",
            "--macos-latency-log", "--msg-level=all=info,latency=info", "--msg-time", "--log-file=" + log]
    args += ["--script=" + c + "/Resources/plugins/" + p for p in PLUGINS]
    shots = os.path.join(out, os.path.splitext(os.path.basename(clip))[0])
    args += ["--script=" + HERE + "/drive.lua", "--script-opts=compare_latency-cycles=%d,compare_latency-gap=%g,compare_latency-shots=%s" % (cycles, gap, shots), clip]
    if os.path.exists(log):
        os.remove(log)
    subprocess.run(args, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=300)
    return log


def summarize(log):
    switches, frames = [], []
    run_line = ""
    for line in open(log, errors="replace"):
        m = re.search(r"switch (on|off) (playing|paused) ([0-9.]+)", line)
        if m:
            switches.append((m.group(1), m.group(2), float(m.group(3))))
        m = re.search(r"frame=\d+ applied=([0-9.]+) newest=[0-9.]+ begin=([0-9.]+) drawable=[0-9.]+ submit=[0-9.]+ shown=([0-9.]+)", line)
        if m:
            frames.append(tuple(float(x) for x in m.groups()))
        if "run: " in line:
            run_line = line.split("run: ", 1)[1].strip()
    rows = []
    for i, (what, state, t) in enumerate(switches):
        first = next((f for f in frames if abs(f[0] - t) < 5e-7), None)
        if not first:
            rows.append((i, what, state, None, None))
            continue
        rows.append((i, what, state, (first[1] - t) * 1000, (first[2] - t) * 1000 if first[2] > 0 else None))
    return run_line, rows


def report(name, run_line, rows):
    print(f"== {name}: {run_line}")
    if not rows:
        print("   no switches logged (did the clip open? is the screen locked?)")
        return
    i, what, state, b, s = rows[0]
    print(f"   first switch ({what}, {state}): begin {b and round(b, 1)} ms, shown {s and round(s, 1)} ms")
    for state in ("playing", "paused"):
        for what in ("on", "off"):
            vals = [r for r in rows[1:] if r[1] == what and r[2] == state]
            shown = sorted(r[4] for r in vals if r[4] is not None)
            begin = sorted(r[3] for r in vals if r[3] is not None)
            missing = sum(1 for r in vals if r[4] is None)
            if not shown:
                print(f"   {state:8} {what:3}: no frame shown with it ({len(vals)} switches)")
                continue
            print(f"   {state:8} {what:3}: shown median {statistics.median(shown):6.1f} ms, max {max(shown):6.1f} ms; "
                  f"drawing began after {statistics.median(begin):5.1f} ms (n={len(shown)}{', %d never shown' % missing if missing else ''})")


def main():
    argv = sys.argv[1:]
    cycles, gap = 10, 0.6
    if "--cycles" in argv:
        k = argv.index("--cycles"); cycles = int(argv[k + 1]); del argv[k:k + 2]
    if "--gap" in argv:
        k = argv.index("--gap"); gap = float(argv[k + 1]); del argv[k:k + 2]
    if len(argv) < 3:
        sys.exit(__doc__)
    app, out, clips = argv[0], os.path.realpath(argv[1]), argv[2:]
    if b'"CGSSessionScreenIsLocked"=Yes' in subprocess.run(["ioreg", "-n", "Root", "-d1"], capture_output=True).stdout:
        sys.exit("The screen is locked: gpu-next would hang")
    os.makedirs(out, exist_ok=True)
    clone = make_clone(app, out)
    for clip in clips:
        log = run_clip(clone, out, clip, cycles, gap)
        report(os.path.basename(clip), *summarize(log))


if __name__ == "__main__":
    main()
