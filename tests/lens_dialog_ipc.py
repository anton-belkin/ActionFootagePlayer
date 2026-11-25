#!/usr/bin/env python3
"""The player's lens dialog (side-panel/lensdialog.lua, gyroflow/measure.lua), headless: the installed app cloned with
this checkout's plugins and vs-gyroflow's release binaries, a clip with its .afp-motion copied in, vo=null. It opens
Source, the dialog (Choose…), runs a Quick Find from it, then Enter: the lens kept for the mode (in the rig's own lens
folder, AFP_CURATED_LENSES: not the user's) and the clip measured again. Pictures of each step in <out>, as the panel
dumps itself over the frame. Nothing is written beside the original clip.

  tests/lens_dialog_ipc.py <repo> <out folder> <folder with GOPR1585.MP4 and its .afp-motion>
"""
import os, sys, time, glob, shutil, subprocess
REPO, OUT, CLIPDIR = sys.argv[1], sys.argv[2], sys.argv[3]
sys.path.insert(0, REPO + "/tests")
import afp_motion_ipc as R
from gyroflow_reload_ipc import Player
rig = os.path.realpath(__import__("tempfile").mkdtemp(prefix="afp-panel-"))
sock = "/tmp/afppanel-%d.sock" % os.getpid()
subprocess.run(["cp", "-cR", os.path.expanduser("~/Applications/ActionFootagePlayer.app"), rig + "/app.app"], check=True)
plugins = rig + "/app.app/Contents/Resources/plugins"
for p in R.PLUGINS:
    src = os.path.join(REPO, "mpv/scripts", p)
    if os.path.isdir(src) and os.path.isdir(os.path.join(plugins, p)):
        for f in glob.glob(os.path.join(src, "*.lua")): shutil.copy(f, os.path.join(plugins, p))
for b in R.BINARIES: shutil.copy(os.path.join(REPO, "vs-gyroflow/target/release", b), plugins + "/gyroflow")
g = plugins + "/gyroflow/main.lua"; m = open(g).read()
m = m.replace('"/usr/bin/open"', '"/usr/bin/true"').replace('if vo ~= "" and not vo:find("gpu-next", 1, true) then return false end', 'if vo ~= "" and vo ~= "null" and not vo:find("gpu-next", 1, true) then return false end').replace('(vo == "" or vo:find("gpu-next", 1, true))', '(vo == "" or vo == "null" or vo:find("gpu-next", 1, true))')
open(g, "w").write(m)
os.makedirs(rig + "/clips")
for f in glob.glob(CLIPDIR + "/GOPR1585.*"): shutil.copy(f, rig + "/clips/")
os.environ["AFP_CURATED_LENSES"] = rig + "/lenses-own/curated_lenses.json"
os.makedirs(rig + "/lenses-own", exist_ok=True)
pl = Player(rig, sock, rig + "/clips/GOPR1585.MP4", "gpu")
def shot(name):
    ass, png = OUT + "/" + name + ".ass", OUT + "/" + name + ".png"
    if os.path.exists(ass): os.remove(ass)
    pl.cmd("script-message-to", "side_panel", "dump", ass, "1920", "1080", "1")
    for _ in range(30):
        if os.path.exists(ass): break
        time.sleep(0.1)
    pl.cmd("sub-add", ass, "select"); time.sleep(0.5)
    pl.cmd("screenshot-to-file", png, "subtitles"); pl.cmd("sub-remove"); print("shot", png, flush=True)
try:
    pl.cmd("set_property", "pause", True)
    pl.cmd("script-message-to", "side_panel", "tab", "source"); time.sleep(1.5)
    shot("player-after-source")
    pl.cmd("script-message-to", "gyroflow", "lens-dialog")
    pl.cmd("script-message-to", "side_panel", "popover", "lens-dialog"); time.sleep(2.5)
    shot("player-after-dialog")
    pl.cmd("script-message-to", "gyroflow", "lens-find"); time.sleep(3.0)
    shot("player-after-finding")
    for _ in range(120):
        j = pl.prop("user-data/gyroflow/lens_job")
        if isinstance(j, dict) and j.get("state") in ("done", "failed"): break
        time.sleep(0.5)
    print("job", pl.prop("user-data/gyroflow/lens_job"), flush=True)
    time.sleep(2.5)
    shot("player-after-found")
    l = pl.prop("user-data/gyroflow/lenses") or {}
    print("lenses", (l.get("found") if isinstance(l, dict) else l), flush=True)
    # Enter: the lens chosen (the one in use: the first that fits) used, the clip measured again
    pl.cmd("keypress", "ENTER"); time.sleep(1.0)
    for _ in range(80):
        if os.path.exists(rig + "/clips/GOPR1585.afp-motion") and os.path.getmtime(rig + "/clips/GOPR1585.afp-motion") > time.time() - 60 and not pl.prop("user-data/gyroflow/measuring"): break
        time.sleep(0.5)
    print("measuring", pl.prop("user-data/gyroflow/measuring"), "lenses-own", os.listdir(rig + "/lenses-own"), flush=True)
    import json
    print("motion", json.loads(open(rig + "/clips/GOPR1585.afp-motion").readline()).get("readout_ms"), flush=True)
    print("log tail:", subprocess.run(["grep", "-a", "-i", "lens\\|measur", rig + "/mpv.log"], capture_output=True, text=True).stdout.splitlines()[-6:], flush=True)
finally:
    pl.quit(); shutil.rmtree(rig, ignore_errors=True)
