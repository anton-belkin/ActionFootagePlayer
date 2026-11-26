#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The looks to HLG the stream renders a log clip with: one for every log curve the player plays (registry.json's
curves with `log`), written where the dlogm plugin has it (the curve's `hdr`, under the plugin's folder), as the
packagers (install.sh, tools/make_app.sh) bake them: the curve's LUT (luts/<lut>, else made from its maker's formulas
by tools/make_hdr_lut.py) into its template (shaders/<id>_hlg.glsl.in where the curve has its own, else
shaders/log_hlg.glsl.in) by tools/cube_to_glsl.py. And the underwater colour's (the curve uw, not log: its shader as it is, shaders/
underwater_hlg.glsl, its numbers set per frame by the stream). The NAS Helper's image runs it (tools/nas-helper/Dockerfile).

    make_looks.py <dlogm plugin folder>      (from anywhere: the repository's files are found from here)
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

TOP = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def log_curves():
    with open(os.path.join(TOP, "mpv", "scripts", "workspace", "registry.json")) as f:
        return [c for c in json.load(f)["curves"] if c.get("log")]


def uw_curve():
    with open(os.path.join(TOP, "mpv", "scripts", "workspace", "registry.json")) as f:
        return next(c for c in json.load(f)["curves"] if c["id"] == "uw")


def make(plugin):
    made = []
    # The underwater colour's shader, where the dlogm plugin has it (install.sh puts it there the same way)
    uw = uw_curve()["hdr"]
    os.makedirs(os.path.dirname(os.path.join(plugin, uw)), exist_ok=True)
    shutil.copyfile(os.path.join(TOP, "shaders", os.path.basename(uw)), os.path.join(plugin, uw))
    made.append(os.path.join(plugin, uw))
    with tempfile.TemporaryDirectory() as tmp:
        for c in log_curves():
            lut = os.path.join(TOP, "luts", c["lut"]) if c.get("lut") else None
            if not lut or not os.path.exists(lut):
                lut = os.path.join(tmp, f"{c['id']}_hlg.cube")
                subprocess.run([sys.executable, "-I", os.path.join(TOP, "tools", "make_hdr_lut.py"), c["id"], lut], check=True,
                               stdout=subprocess.DEVNULL)
            template = os.path.join(TOP, "shaders", f"{c['id']}_hlg.glsl.in")
            if not os.path.exists(template):
                template = os.path.join(TOP, "shaders", "log_hlg.glsl.in")
            out = os.path.join(plugin, c["hdr"])
            os.makedirs(os.path.dirname(out), exist_ok=True)
            subprocess.run([sys.executable, "-I", os.path.join(TOP, "tools", "cube_to_glsl.py"), template, lut, out], check=True,
                           stdout=subprocess.DEVNULL)
            made.append(out)
    return made


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    for p in make(sys.argv[1]):
        print(p)
