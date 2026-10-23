"""ActionFootagePlayer's setup on Windows, in tools\\ of the app, run with the app's own Python (macos/setup.sh is the
Mac's). The installer runs it, and ActionFootagePlayer.exe in the background while DJI's LUT isn't there.

  setup.py lut [<cube>]   DJI's D-Log M -> Rec.709 LUT, and the sdr shader made from it, into
                          %LOCALAPPDATA%\\ActionFootagePlayer\\dlogm\\ (the dlogm plugin's DLOGM_DATA). DJI's LUT isn't
                          redistributed: it's downloaded from DJI and checked, or <cube> is used. The URL and the
                          checksum are macos/setup.sh's (dji_lut.json, written by tools/make_windows_app.sh)
"""
import hashlib
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
CREATE_NO_WINDOW = 0x08000000


def data_dir():
    data = os.environ.get("DLOGM_DATA")
    if data:
        return Path(data)
    return Path(os.environ.get("LOCALAPPDATA", Path.home())) / "ActionFootagePlayer" / "dlogm"


def lut_ok(path, sha256):
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest() == sha256
    except OSError:
        return False


def fetch_lut(out, source):
    if lut_ok(out, source["sha256"]):
        return
    part = out.with_name(out.name + ".part")
    try:
        with urllib.request.urlopen(source["url"], timeout=60) as response:
            part.write_bytes(response.read())
    except OSError as e:
        part.unlink(missing_ok=True)
        sys.exit(f"Couldn't download DJI's LUT: {e}")
    if not lut_ok(part, source["sha256"]):
        part.unlink(missing_ok=True)
        sys.exit("The LUT DJI serves isn't the one this was made for (checksum differs)")
    part.replace(out)


def lut(given=None):
    source = json.loads((TOOLS / "dji_lut.json").read_text())
    dlogm = data_dir()
    cube = dlogm / "luts" / "dji-dlogm-rec709.cube"
    shader = dlogm / "shaders" / "dlogm_lut3d.glsl"
    cube.parent.mkdir(parents=True, exist_ok=True)
    shader.parent.mkdir(parents=True, exist_ok=True)
    if given and not lut_ok(cube, source["sha256"]):
        given = Path(given)
        if not lut_ok(given, source["sha256"]):
            sys.exit(f"{given} isn't DJI's LUT (checksum differs)")
        cube.write_bytes(given.read_bytes())
    fetch_lut(cube, source)
    if not shader.is_file() or cube.stat().st_mtime > shader.stat().st_mtime:
        part = shader.with_name(shader.name + ".part")
        # Without a console window: run by pythonw, a python.exe of its own would open one
        done = subprocess.run([sys.executable, "-I", str(TOOLS / "cube_to_glsl.py"),
                               str(TOOLS / "dlogm_lut3d.glsl.in"), str(cube), str(part)],
                              stdout=subprocess.DEVNULL, creationflags=CREATE_NO_WINDOW)
        if done.returncode != 0:
            part.unlink(missing_ok=True)
            sys.exit("Couldn't make the shader from DJI's LUT")
        part.replace(shader)
    print(f"DJI's Rec.709 LUT: {dlogm}")


def main():
    args = sys.argv[1:]
    if args[:1] == ["lut"] and len(args) <= 2:
        lut(args[1] if len(args) == 2 else None)
    else:
        sys.exit("usage: setup.py lut [<cube>]")


if __name__ == "__main__":
    main()
