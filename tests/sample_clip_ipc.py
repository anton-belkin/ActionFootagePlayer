#!/usr/bin/env python3
"""The sample clip as a newcomer opens it (tools/make_sample_clip.py; docs/dev/release.md), in the app's own mpv with
its plugins, headless (--vo=null: the warp is parsed, not drawn), over the JSON IPC, with the app's defaults (an
empty gyroflow.conf). Passes when, under the name the welcome card gives it:

  - the gyroflow plugin sets its warp (the stabilization from the camera's motion data, the GPU path);
  - the settings it plays with have the denoise on and the clip read as under water;
  - the dlogm plugin converts it from D-Log M;
  - the frame is 1920x1080 at 59.94 fps.

  tests/sample_clip_ipc.py [--keep] <ActionFootagePlayer.app> <Sample dive - Osmo Action 6.mp4>
"""
import json, os, shutil, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gyroflow_reload_ipc import make_rig, Player, REPO  # noqa: E402

failures = 0


def check(what, ok, seen):
    global failures
    failures += 0 if ok else 1
    print(("ok   " if ok else "FAIL ") + what + f": {seen!r}")
    sys.stdout.flush()


def main():
    args = sys.argv[1:]
    keep = "--keep" in args
    args = [a for a in args if a != "--keep"]
    if len(args) != 2:
        sys.exit(__doc__)
    app, clip = args
    rig, sock = make_rig(app, clip, None, os.path.join(REPO, "mpv", "scripts", "gyroflow"))
    named = os.path.join(rig, "clips", "Sample dive - Osmo Action 6.mp4")
    shutil.copy(clip, named)
    pl = Player(rig, sock, named)
    try:
        time.sleep(2)
        check("stabilized (the warp set)", pl.state()["warp"] is not None, pl.state())
        s = pl.prop("user-data/gyroflow/settings") or {}
        check("denoise on", s.get("denoise") is True, {k: s.get(k) for k in ("denoise", "denoise_strong", "temporal_denoise")})
        check("under water", s.get("underwater") in (True, "yes", "auto") or s.get("in_water") is True,
              {k: s.get(k) for k in ("underwater", "in_water") if k in s})
        curve = pl.prop("user-data/dlogm/curve")
        check("D-Log M converted", curve == "dlogm", curve)
        size = (pl.prop("width"), pl.prop("height"), round(pl.prop("container-fps") or 0, 2))
        check("1920x1080 at 59.94", size == (1920, 1080, 59.94), size)
        print("settings:", json.dumps(s, sort_keys=True)[:1500])
    finally:
        pl.quit()
        if keep:
            print("rig kept:", rig)
        else:
            shutil.rmtree(rig, ignore_errors=True)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
