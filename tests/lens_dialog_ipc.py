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
        for f in glob.glob(os.path.join(src, "*.lua")) + glob.glob(os.path.join(src, "*.json")): shutil.copy(f, os.path.join(plugins, p))
for b in R.BINARIES: shutil.copy(os.path.join(REPO, "vs-gyroflow/target/release", b), plugins + "/gyroflow")
for g in glob.glob(plugins + "/gyroflow/*.lua"):
  m = open(g).read()
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
    # Try on this clip: another lens of the mode (the 29.97 built-in) given to this clip alone, the clip measured again
    # with it; then forgotten (back to the mode's)
    def measured_lens():
        return json.loads(open(rig + "/clips/GOPR1585.afp-motion").readline()).get("lens_name")
    for _ in range(120):
        if not pl.prop("user-data/gyroflow/measuring"): break
        time.sleep(0.5)
    before = os.path.getmtime(rig + "/clips/GOPR1585.afp-motion")
    pl.cmd("script-message-to", "gyroflow", "lens-dialog"); time.sleep(2.0)
    l = pl.prop("user-data/gyroflow/lenses") or {}
    other = [r for r in l.get("lenses", []) if "29.97fps, built in" in r.get("name", "") and "assign" in (r.get("use") or {})]
    assert other, "the 29.97 built-in lens isn't offered"
    pl.cmd("script-message-to", "gyroflow", "lens-try", "assign", other[0]["use"]["assign"])
    for _ in range(120):
        t = os.path.getmtime(rig + "/clips/GOPR1585.afp-motion")
        if t > before and not pl.prop("user-data/gyroflow/measuring") and "29.97" in (measured_lens() or ""): break
        time.sleep(0.5)
    own = json.load(open(rig + "/clips/GOPR1585.afp.json"))["source"].get("lens", {})
    print("tried on the clip:", own.get("name"), "| measured with:", measured_lens(), flush=True)
    assert "29.97" in (own.get("name") or "") and "29.97" in (measured_lens() or ""), "not measured with the lens tried"
    time.sleep(2.0)
    pl.cmd("script-message-to", "side_panel", "popover", ""); time.sleep(0.5)
    shot("player-after-tried-source")
    pl.cmd("script-message-to", "gyroflow", "lens-dialog")
    pl.cmd("script-message-to", "side_panel", "popover", "lens-dialog"); time.sleep(2.5)
    shot("player-after-tried-dialog")
    pl.cmd("script-message-to", "side_panel", "popover", "")
    before = os.path.getmtime(rig + "/clips/GOPR1585.afp-motion")
    pl.cmd("script-message-to", "gyroflow", "lens-forget", "clip")
    for _ in range(120):
        if os.path.getmtime(rig + "/clips/GOPR1585.afp-motion") > before and not pl.prop("user-data/gyroflow/measuring") and "29.97" not in (measured_lens() or ""): break
        time.sleep(0.5)
    own = json.load(open(rig + "/clips/GOPR1585.afp.json"))["source"].get("lens")
    print("forgotten:", own is None, "| measured with:", measured_lens(), flush=True)
    assert own is None and "29.97" not in (measured_lens() or ""), "the clip's own lens wasn't taken away"
    print("log tail:", subprocess.run(["grep", "-a", "-i", "lens\\|measur", rig + "/mpv.log"], capture_output=True, text=True).stdout.splitlines()[-6:], flush=True)
finally:
    pl.quit(); shutil.rmtree(rig, ignore_errors=True)
