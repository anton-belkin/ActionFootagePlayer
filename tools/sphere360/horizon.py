#!/usr/bin/env python3
# tools/sphere360/horizon.py <folder>: how a 360 camera's motion data turns into its rig's axes, from the horizon in
# the plugin's sphere (vs-gyroflow/src/sphere360.rs `imu_turn`). The sphere is the rig's own, made from the lens
# calibration alone, so the sky's lower edge in it, fitted with a great circle, is the camera's up in the rig's axes,
# every second, with no IMU in it. Then:
#   - the gyro's turn: of the 24 axis turns, the one with which the gyro alone and a single vertical explain those ups;
#   - the accelerometer's: the one that reads those ups best (it leans with a motorcycle in a curve: look at the median);
#   - with up.txt, how far the plugin's own stamps are from them (its camera's up per second).
# The folder holds, from the ignored tests in vs-gyroflow (AFP_360_CLIP the camera's file):
#   sphere-<t>.png  AFP_360_OUT=<folder> AFP_360_WIDTH=2048 AFP_360_TIMES=1,2,...  cargo test --release --lib -- \
#                   sphere360::tests::a_clip --ignored --nocapture 2>&1 | grep '^UP' > <folder>/up.txt
#   imu.txt         cargo test --release --lib -- imu_frames::gyro_by_second --ignored --nocapture 2>&1 \
#                   | grep -E '^(GYRO|ACC)' > <folder>/imu.txt
# Needs numpy and Pillow. A clip with sky over at least part of the horizon most of the time (outdoors).
import itertools, os, re, sys
import numpy as np
from PIL import Image

d = sys.argv[1]
AXES = [np.array(v, float) for v in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (-1, 0, 0), (0, -1, 0), (0, 0, -1)]]


def turns():
    for a, b in itertools.product(AXES, AXES):
        c = np.cross(a, b)
        if np.linalg.norm(c) > 0.5:
            yield np.stack([a, b, c], 1), a, b


def direction(x, y, w, h):
    lon = ((x + 0.5) / w - 0.5) * 2 * np.pi
    lat = (0.5 - (y + 0.5) / h) * np.pi
    return np.array([np.cos(lat) * np.sin(lon), -np.sin(lat), np.cos(lat) * np.cos(lon)])


def horizon(path):
    im = np.asarray(Image.open(path).convert("RGB")).astype(float)
    w = im.shape[1]
    h = w // 2
    r, g, b = im[:h, :, 0], im[:h, :, 1], im[:h, :, 2]
    sky = (b > 150) & (b > r + 25) & (b > g + 5)
    pts = []
    for x in range(0, w, 4):
        col = sky[:, x]
        if not col[:h // 25].any():
            continue
        y = 0
        while y < h and col[y]:
            y += 1
        if h // 16 < y < h - h // 16:
            pts.append(direction(x, y, w, h))
    pts = np.array(pts)
    if len(pts) < 50:
        return None
    best, count = None, 0
    rng = np.random.default_rng(1)
    for _ in range(400):
        p, q = pts[rng.choice(len(pts), 2, replace=False)]
        n = np.cross(p, q)
        if np.linalg.norm(n) < 1e-3:
            continue
        n /= np.linalg.norm(n)
        inliers = np.abs(pts @ n) < np.sin(np.radians(3))
        if inliers.sum() > count:
            best, count = inliers, inliers.sum()
    n = np.linalg.svd(pts[best])[2][2]
    return -n if n[1] > 0 else n


def angle(a, b):
    return np.degrees(np.arccos(np.clip(a @ b / np.linalg.norm(a) / np.linalg.norm(b), -1, 1)))


ups = {}
for f in sorted(os.listdir(d)):
    m = re.match(r"sphere-(\d+)\.png$", f)
    if m and (u := horizon(os.path.join(d, f))) is not None:
        ups[int(m[1])] = u
gyro, acc = {}, {}
for line in open(os.path.join(d, "imu.txt")):
    p = line.split()
    if p[0] == "GYRO":
        gyro[int(p[1])] = np.array(list(map(float, p[2:11]))).reshape(3, 3)
    elif p[0] == "ACC":
        acc[int(p[1])] = np.array(list(map(float, p[2:5])))
for t in sorted(ups):
    print(f"t={t} up={np.round(ups[t], 3)} tilt {np.degrees(np.arccos(-ups[t][1])):.1f}°")

ts = sorted(set(ups) & set(gyro))
res = []
for G, a, b in turns():
    w = sum(gyro[t] @ G.T @ ups[t] for t in ts)
    w /= np.linalg.norm(w)
    e = [angle(G @ gyro[t].T @ w, ups[t]) for t in ts]
    res.append((np.sqrt(np.mean(np.square(e))), max(e), a, b))
res.sort(key=lambda r: r[0])
print("the gyro's turn (images of x and y), the horizon explained by it and one vertical:")
for r in res[:3]:
    print(f"  rms {r[0]:.1f}° max {r[1]:.1f}°  x->{r[2]} y->{r[3]}")

ts = sorted(set(ups) & set(acc))
res = []
for M, a, b in turns():
    e = [angle(M @ acc[t], ups[t]) for t in ts]
    res.append((np.median(e), np.mean(e), a, b))
res.sort(key=lambda r: r[0])
print("the accelerometer's turn, its reading against the horizon:")
for r in res[:3]:
    print(f"  median {r[0]:.1f}° mean {r[1]:.1f}°  x->{r[2]} y->{r[3]}")

path = os.path.join(d, "up.txt")
if os.path.exists(path):
    stamped = {int(float(l.split()[1])): np.array(list(map(float, l.split()[2:5]))) for l in open(path) if l.startswith("UP")}
    e = [angle(stamped[t], ups[t]) for t in sorted(set(stamped) & set(ups))]
    print(f"the plugin's stamps against the horizon: rms {np.sqrt(np.mean(np.square(e))):.1f}° max {max(e):.1f}°  "
          + " ".join(f"{x:.0f}" for x in e))
