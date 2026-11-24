#!/usr/bin/env python3
# The underwater colour (#273) in gyroflow-export against the player's own: on real clips, by hand (it needs clips and
# the app's ffmpeg with libplacebo, so it isn't in release.yml). For each clip and time:
#   1. the numbers: the frame grabbed as the player's uwcolor.lua grabs it, measured by its own Lua rule
#      (mpv/scripts/dlogm/uw_estimate.lua) -> what the exporter's photo says it measured
#   2. the photo's HDR (gyroflow-export --photo, AFP_PHOTO_RENDITIONS) -> the player's shader
#      (shaders/underwater_hlg.glsl) run by libplacebo with those numbers, as mpv runs it on MAIN
#   3. a video export (HLG ProRes: no HEVC loss to hide in; the pipes' GPU LUT, --uw-params the same numbers) -> the
#      same reference at that frame. Its 4:2:0 (each chroma sample the mean of 2x2 pixels made HLG) is looser than the
#      photo's 4:4:4 on noisy footage, as for every curve's export
#   4. a video export as it plays (numbers measured and eased), and an SDR (NBCU) one: made, tagged right
# Colours are compared as 10-bit HLG codes: everywhere (median, 99th percentile of |diff|) and at 9 points (a 3x3 grid,
# each the mean of 16x16 pixels, as R G B codes side by side).
#
#   python3 tests/uw_export_check.py --exporter vs-gyroflow/target/release/gyroflow-export \
#       --app ~/Applications/ActionFootagePlayer.app <clip>@<seconds>...
# Writes nothing outside --work (default: a temporary folder); the clips are only read.

import argparse, json, os, re, shutil, subprocess, sys, tempfile
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)


def run(cmd, env=None, check=True):
    r = subprocess.run(cmd, env=env, capture_output=True, text=True)
    if check and r.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, cmd))}\n{r.stderr[-2000:]}")
    return r


def lua_estimate(bgr0):
    code = f"""
package.path = "{REPO}/mpv/scripts/dlogm/?.lua;" .. package.path
local E = require 'uw_estimate'
local f = assert(io.open("{bgr0}", "rb")); local d = f:read("*a"); f:close()
local R, G, B = E.grid_from_bgr0(d, 960, 540, 960 * 4, 320, 180)
local p = E.estimate(R, G, B, 320, 180)
print(string.format("%.4f %.4f %.4f %.4f", p.dr, p.db, p.k, p.ev))
"""
    out = run(["luajit", "-e", code]).stdout.split()
    return [float(x) for x in out]


def reference(ffmpeg, env, clip, t, params, strength, highlights, work, size):
    """The player's shader on the frame at t, by libplacebo (MAIN: the decoded RGB, tagged HLG as dlogm tags it)"""
    text = open(os.path.join(REPO, "shaders/underwater_hlg.glsl")).read()
    values = {"uw_dr": params[0], "uw_db": params[1], "uw_k": params[2], "uw_ev": params[3], "uw_s": strength,
              "uw_hp": 2.5 if highlights else 1.0}
    for name, v in values.items():
        text = re.sub(rf"(//!PARAM {name}\n(?://!.*\n)*)[^\n]*\n", lambda m: m.group(1) + f"{v:.6f}\n", text)
    shader = os.path.join(work, "uw_ref.glsl")
    open(shader, "w").write(text)
    raw = os.path.join(work, "ref.raw")
    run([ffmpeg, "-v", "error", "-nostdin", "-init_hw_device", "vulkan", "-ss", f"{t:.3f}", "-i", clip, "-frames:v", "1", "-vf",
         "setparams=color_primaries=bt2020:color_trc=arib-std-b67,"
         "libplacebo=format=gbrp16le:color_primaries=bt2020:color_trc=arib-std-b67:range=pc:dithering=none:"
         f"custom_shader_path={shader}", "-f", "rawvideo", "-y", raw], env=env)
    w, h = size
    g, b, r = np.fromfile(raw, dtype="<u2").reshape(3, h, w).astype(np.float64) / 65535.0
    return np.stack([r, g, b], axis=-1)


def decode_frame(ffmpeg, path, index, size):
    """Frame `index` of an export, as RGB in its own matrix (HLG BT.2020's), 0..1"""
    raw = path + ".frame.raw"
    run([ffmpeg, "-v", "error", "-nostdin", "-i", path, "-vf", f"select=eq(n\\,{index}),scale=out_range=pc:in_color_matrix=bt2020:flags=accurate_rnd+full_chroma_int",
         "-frames:v", "1", "-pix_fmt", "gbrp16le", "-f", "rawvideo", "-y", raw])
    w, h = size
    g, b, r = np.fromfile(raw, dtype="<u2").reshape(3, h, w).astype(np.float64) / 65535.0
    return np.stack([r, g, b], axis=-1)


def colour_tags(tools, path):
    s = json.loads(run([os.path.join(tools, "ffprobe"), "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=color_transfer,color_primaries", "-of", "json", path]).stdout)["streams"][0]
    return f"{s.get('color_primaries')}/{s.get('color_transfer')}"


def compare(what, got, ref, limit_median, limit_point):
    d = np.abs(got - ref) * 1023
    h, w, _ = ref.shape
    pts = []
    for fy in (0.2, 0.5, 0.8):
        for fx in (0.2, 0.5, 0.8):
            y, x = int(fy * h), int(fx * w)
            a = got[y:y + 16, x:x + 16].reshape(-1, 3).mean(0) * 1023
            b = ref[y:y + 16, x:x + 16].reshape(-1, 3).mean(0) * 1023
            pts.append(f"({fy:.1f},{fx:.1f}) {a[0]:5.0f}{a[1]:5.0f}{a[2]:5.0f} | {b[0]:5.0f}{b[1]:5.0f}{b[2]:5.0f}")
    med, p99 = float(np.median(d)), float(np.percentile(d, 99))
    pt_max = max(float(np.abs((got[int(fy * h):int(fy * h) + 16, int(fx * w):int(fx * w) + 16].reshape(-1, 3).mean(0)
                               - ref[int(fy * h):int(fy * h) + 16, int(fx * w):int(fx * w) + 16].reshape(-1, 3).mean(0))).max() * 1023)
                 for fy in (0.2, 0.5, 0.8) for fx in (0.2, 0.5, 0.8))
    ok = med <= limit_median and pt_max <= limit_point
    print(f"  {what}: |diff| in 10-bit codes median {med:.2f}, p99 {p99:.2f}, worst point {pt_max:.2f} (at most {limit_median}, {limit_point}) -> {'OK' if ok else 'FAIL'}")
    print("     point       export R G B     | player's shader R G B")
    for p in pts: print("     " + p)
    return ok


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exporter", required=True)
    ap.add_argument("--app", default=os.path.expanduser("~/Applications/ActionFootagePlayer.app"))
    ap.add_argument("--work")
    ap.add_argument("--strength", type=float, default=1.0)
    ap.add_argument("--highlights", action="store_true")
    ap.add_argument("items", nargs="+", help="<clip>@<seconds>")
    a = ap.parse_args()
    contents = os.path.join(a.app, "Contents")
    ffmpeg = os.path.join(contents, "Helpers/ffmpeg")
    work = a.work or tempfile.mkdtemp(prefix="uw-check-")
    os.makedirs(work, exist_ok=True)
    tools = os.path.join(work, "tools")
    os.makedirs(tools, exist_ok=True)
    for name, src in (("ffmpeg", ffmpeg), ("ffprobe", os.path.join(contents, "Helpers/ffprobe")),
                      ("afp-photo", os.path.join(contents, "Resources/plugins/gyroflow/afp-photo"))):
        if not os.path.exists(os.path.join(tools, name)): os.symlink(src, os.path.join(tools, name))
    icd = os.path.join(contents, "Resources/vulkan/icd.d/MoltenVK_icd.json")
    env = dict(os.environ, HOME=os.path.join(work, "home"), GYROFLOW_TOOLS=tools, VK_ICD_FILENAMES=icd, VK_DRIVER_FILES=icd,
               DLOGM_DATA=os.path.join(contents, "Resources/plugins/dlogm"), AFP_PHOTO_RENDITIONS=os.path.join(work, "rend"))
    os.makedirs(env["HOME"], exist_ok=True)
    os.makedirs(env["AFP_PHOTO_RENDITIONS"], exist_ok=True)
    uw = ["--curve", "uw", "--no-stabilize", "--uw-strength", f"{a.strength}", "--uw-highlights", "yes" if a.highlights else "no"]
    failed = 0
    for item in a.items:
        clip, t = item.rsplit("@", 1)
        t = float(t)
        name = os.path.splitext(os.path.basename(clip))[0]
        print(f"{name} at {t:.2f} s")
        info = json.loads(run([os.path.join(tools, "ffprobe"), "-v", "error", "-select_streams", "v:0", "-show_entries",
                               "stream=width,height,r_frame_rate", "-of", "json", clip]).stdout)["streams"][0]
        size = (info["width"], info["height"])
        num, den = map(int, info["r_frame_rate"].split("/"))
        fps = num / den

        # 1. the numbers
        bgr0 = os.path.join(work, f"{name}.bgr0")
        run([ffmpeg, "-v", "error", "-nostdin", "-hwaccel", "videotoolbox", "-ss", f"{t:.3f}", "-i", clip, "-map", "0:v:0", "-frames:v", "1",
             "-vf", "scale=960:540:flags=area", "-f", "rawvideo", "-pix_fmt", "bgr0", "-y", bgr0])
        lua = lua_estimate(bgr0)
        out = os.path.join(work, "out")
        r = run([a.exporter, "--photo", f"{t}", "--radius", "0", "--force", "--out", out] + uw + [clip], env=env)
        m = re.search(r"underwater colour \(red ([-+\d.]+), blue ([-+\d.]+), leak ([-+\d.]+), lift ([-+\d.]+) EV", r.stderr)
        got = [float(x) for x in m.groups()] if m else None
        same = got is not None and max(abs(x - y) for x, y in zip(got, lua)) <= 1.5e-4
        print(f"  numbers: player's rule {lua}, the photo's {got} -> {'OK' if same else 'FAIL'}")
        failed += not same
        params = got or lua

        # 2. the photo's HDR
        ref = reference(ffmpeg, env, clip, t, params, a.strength, a.highlights, work, size)
        rend = [f for f in os.listdir(env["AFP_PHOTO_RENDITIONS"]) if f.startswith(f"{name}_{t:.2f}s") and ".hdr." in f]
        photo = np.fromfile(os.path.join(env["AFP_PHOTO_RENDITIONS"], sorted(rend)[-1]), dtype="<u2").reshape(size[1], size[0], 3) / 65535.0
        failed += not compare("photo HDR vs the player's shader", photo, ref, 2.5, 4.0)

        # 3. a video export of the same numbers, at that frame
        n = round(t * fps)
        frm = max(0.0, (n - 3) / fps)
        video = os.path.join(work, f"{name}_hlg.mov")
        run([a.exporter, "--force", "--output", video, "--from", f"{frm:.4f}", "--to", f"{frm + 0.5:.4f}", "--prores",
             "--uw-params", ",".join(f"{x:.4f}" for x in params), "--gpu", "off"] + uw + [clip], env=env)
        tags = colour_tags(tools, video)
        print(f"  video tags: {tags} -> {'OK' if tags == 'bt2020/arib-std-b67' else 'FAIL'}")
        failed += tags != "bt2020/arib-std-b67"
        first = int(np.ceil(frm * fps - 1e-6))
        frame = decode_frame(ffmpeg, video, n - first, size)
        failed += not compare("video (HLG ProRes, 4:2:0 on the GPU) vs the player's shader", frame, ref, 6.0, 8.0)

        # 4. as it plays, and in SDR
        for extra, label, want in (([], "as it plays", "bt2020/arib-std-b67"), (["--nbcu"], "SDR (NBCU)", "bt709/bt709")):
            v = os.path.join(work, f"{name}_{'sdr' if extra else 'plays'}.mov")
            r = run([a.exporter, "--force", "--output", v, "--from", f"{t:.3f}", "--to", f"{t + 2:.3f}"] + extra + uw + [clip], env=env)
            tags = colour_tags(tools, v)
            line = next((l.strip() for l in r.stderr.splitlines() if "Underwater colour" in l), "")
            ok = tags == want and "measured as it plays" in line
            print(f"  video {label}: {tags}; {line[:150]} -> {'OK' if ok else 'FAIL'}")
            failed += not ok
    print("all OK" if failed == 0 else f"{failed} FAILED")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
