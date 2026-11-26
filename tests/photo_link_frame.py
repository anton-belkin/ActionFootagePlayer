#!/usr/bin/env python3
# A photo link's own frame (Save frame's camera and crop, kept in its link) as the player shows it, against Share's
# render of the link (gyroflow-export --camera --crop): the same picture, by its edges (the looks may differ: the
# player shows the clip's look, the render here is SDR).
#
#   python3 tests/photo_link_frame.py <player's frame (screenshot-to-file … video)> <gyroflow-export's photo> [<control>]
#
# Pass: no shift and an edge correlation well above a control (the same link without its camera: ~0.05). How the two
# were made (2026-11-19, the app's tools and the player headless with the app's plugins and this branch's Lua): a
# link {t 8.0, camera 10,-5,3,1.25, crop 0.15,0.1,0.7,0.75} to a DJI clip (afp-meta <link.jpg> --link …), the
# player opened on it (paused at the frame, the CPU filter: the VapourSynth path of then, since taken out), screenshot-to-file <out> video; gyroflow-export --photo
# 8.0 --format jpeg --curve none --sdr-image nbcu --camera 10,-5,3,1.25 --crop 0.15,0.1,0.7,0.75 <clip>.
# Measured: shift 0,0, edge correlation 0.78; without the camera 0.05, shifted ~700,300 px.

import subprocess, sys
import numpy as np

def edges(path, w=840, h=506):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", f"scale={w}:{h},format=gray", "-f", "rawvideo", "-"],
                         capture_output=True, check=True).stdout
    a = np.frombuffer(raw, np.uint8).reshape(h, w).astype(float)
    gy, gx = np.gradient(a)
    g = np.hypot(gx, gy)
    return (g - g.mean()) / (g.std() + 1e-9)

def compare(a_path, b_path):
    a, b = edges(a_path), edges(b_path)
    f = np.fft.fft2(a) * np.conj(np.fft.fft2(b))
    r = np.real(np.fft.ifft2(f / (np.abs(f) + 1e-9)))
    y, x = np.unravel_index(r.argmax(), r.shape)
    h, w = a.shape
    return (x - w if x > w // 2 else x, y - h if y > h // 2 else y), float(np.mean(a * b))

shift, corr = compare(sys.argv[2], sys.argv[1])
print(f"player vs Share: shift {shift}, edge correlation {corr:.3f}")
ok = abs(shift[0]) <= 2 and abs(shift[1]) <= 2 and corr > 0.5
if len(sys.argv) > 3:
    cshift, ccorr = compare(sys.argv[2], sys.argv[3])
    print(f"control: shift {cshift}, edge correlation {ccorr:.3f}")
    ok = ok and ccorr < corr / 3
print("ok" if ok else "FAIL")
sys.exit(0 if ok else 1)
