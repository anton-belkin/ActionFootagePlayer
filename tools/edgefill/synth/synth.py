# The synthetic bench's shared part: the camera geometry warp.glsl uses (a pinhole source camera: the lens model's
# branch with no coefficients; a rectilinear output), an analytic world, the scenarios, the frames a camera sees, and
# the table the shader reads the cameras from.
#
# Directions: x right, y down, z forward. A camera's orientation Q takes its own rays to the world's (qrot, as the
# shader's). The world's longitude is atan2(x, z), its latitude asin(y) (negative up). The view (the output camera,
# table texel 15) is Qv; the source camera is Qc; each row's rotation in the table is conj(Qc) * Qv (output ray ->
# source ray).
import math
import numpy as np

SRC_W, SRC_H = 3840, 2160          # the kept shader's frame (warp.glsl's SRC_W/SRC_H, OUT_W/OUT_H)
F = 2288.0                          # the source camera's focal length in its pixels: 80 degrees across
FOV = 1.4                           # the output's zoom out (table texel 7.x: the player's --wide)
FPS = 59.940059940                  # the kept shader's FPS
OUT_F = F / FOV
PER_FRAME = 33                      # the kept shader's KNOTS
PARAM_TEXELS = 17
BLOCK = PARAM_TEXELS + PER_FRAME
COLS = 16384 // BLOCK


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (aw * bx + bw * ax + ay * bz - az * by,
            aw * by + bw * ay + az * bx - ax * bz,
            aw * bz + bw * az + ax * by - ay * bx,
            aw * bw - ax * bx - ay * by - az * bz)


def qconj(q):
    return (-q[0], -q[1], -q[2], q[3])


def qrot(q, v):
    """v: (..., 3) array"""
    qv = np.array(q[:3], dtype=np.float64)
    t = 2.0 * np.cross(qv, v)
    return v + q[3] * t + np.cross(qv, t)


def rot_y(a):
    return (0.0, math.sin(a / 2), 0.0, math.cos(a / 2))


def rot_x(a):
    return (math.sin(a / 2), 0.0, 0.0, math.cos(a / 2))


def rot_z(a):
    return (0.0, 0.0, math.sin(a / 2), math.cos(a / 2))


def orient(yaw_deg, pitch_deg, roll_deg=0.0):
    """Looking at longitude yaw; pitch positive up; roll about the line of sight (positive clockwise on screen)"""
    return qmul(qmul(rot_y(math.radians(yaw_deg)), rot_x(math.radians(pitch_deg))), rot_z(math.radians(roll_deg)))


def lonlat(d):
    d = d / np.linalg.norm(d, axis=-1, keepdims=True)
    return np.arctan2(d[..., 0], d[..., 2]), np.arcsin(np.clip(d[..., 1], -1, 1))


def lonlat_dir(lon, lat):
    cl = np.cos(lat)
    return np.stack([cl * np.sin(lon), np.sin(lat), cl * np.cos(lon)], axis=-1)


# ---- the world ----

def _blob(lon, lat, lon0, lat0, s_lon, s_lat):
    dl = (lon - lon0 + np.pi) % (2 * np.pi) - np.pi
    return np.exp(-0.5 * ((dl / s_lon) ** 2 + ((lat - lat0) / s_lat) ** 2))


def world(lon, lat, which=0, gain=1.0):
    """RGB (coded, 0..1) of the world in direction (lon, lat). 0: above the water (sky, horizon, sea with fine waves,
    posts every 30 degrees), 1: under water (blue-green, light shafts, reef)"""
    lon = np.asarray(lon, dtype=np.float64)
    lat = np.asarray(lat, dtype=np.float64)
    out = np.zeros(lon.shape + (3,))
    if which == 0:
        up = np.clip(-lat / 0.9, 0, 1)
        sky = np.stack([0.55 - 0.25 * up, 0.72 - 0.2 * up, 0.92 - 0.05 * up], -1)
        clouds = sum(_blob(lon, lat, math.radians(a), -0.35 - 0.1 * (i % 3), 0.25, 0.08) for i, a in enumerate(range(-170, 180, 40)))
        sky = sky + 0.25 * np.clip(clouds, 0, 1)[..., None]
        down = np.clip(lat / 0.8, 0, 1)
        waves = 0.06 * np.sin(lon * 180.0 + 40.0 * lat) * np.sin(lon * 23.0 + lat * 150.0)
        sea = np.stack([0.10 + 0.05 * down, 0.45 - 0.2 * down, 0.50 - 0.15 * down], -1) + waves[..., None]
        horizon = 0.05
        t = np.clip((lat - horizon) / 0.004 + 0.5, 0, 1)[..., None]
        out = sky * (1 - t) + sea * t
        # an island band on part of the horizon
        isl = (np.abs(((lon - 0.8 + np.pi) % (2 * np.pi)) - np.pi) < 0.35) & (lat > horizon - 0.03) & (lat < horizon)
        out[isl] = (0.35, 0.33, 0.25)
        # posts: every 10 degrees (so the frame's edges keep crossing them), 1.2 degrees wide, from above the horizon
        # down into the sea, colors in turn
        cols = np.array([(0.9, 0.15, 0.1), (0.95, 0.85, 0.2), (0.95, 0.95, 0.95), (0.12, 0.12, 0.14), (0.2, 0.75, 0.25)])
        k = np.floor((lon + np.pi) / math.radians(10)).astype(int)
        mid = -np.pi + (k + 0.5) * math.radians(10)
        post = (np.abs(lon - mid) < math.radians(0.6)) & (lat > -0.30 + 0.05 * (k % 3)) & (lat < 0.40)
        out[post] = cols[k[post] % 5]
    else:
        up = np.clip(-lat / 0.9, -1, 1)
        base = np.stack([0.03 + 0.05 * up, 0.25 + 0.15 * up, 0.35 + 0.15 * up], -1)
        shafts = 0.08 * np.clip(np.sin(lon * 9.0 + 0.4 * lat), 0, 1) ** 4 * np.clip(-lat + 0.2, 0, 1)
        base = base + shafts[..., None]
        reef = sum(_blob(lon, lat, math.radians(a), 0.45 + 0.08 * ((i * 7) % 3), 0.18, 0.12) for i, a in enumerate(range(-175, 180, 25)))
        reef = np.clip(reef, 0, 1) * np.clip((lat - 0.15) / 0.2, 0, 1)
        tex = 0.5 + 0.5 * np.sin(lon * 120.0) * np.sin(lat * 110.0)
        coral = np.stack([0.55 + 0.2 * tex, 0.35 + 0.1 * tex, 0.25 + 0.05 * tex], -1)
        out = base * (1 - reef[..., None]) + coral * reef[..., None]
    return np.clip(out * gain, 0, 1)


# ---- scenarios: per frame (view yaw, view pitch, camera yaw, camera pitch) in degrees, the world, its gain, and
# optionally (view roll, camera roll); path() gives all eight ----

def _ease(x):
    x = np.clip(x, 0, 1)
    return x * x * (3 - 2 * x)


def _shake(t, amp=6.0, seed=0):
    ph = [0.3, 1.1, 2.0][seed % 3]
    return amp * (np.sin(2 * np.pi * 1.3 * t + ph) * 0.7 + np.sin(2 * np.pi * 3.7 * t + 2 * ph) * 0.3)


SCENARIOS = {}


def scenario(name, seconds, doc):
    def wrap(fn):
        SCENARIOS[name] = (fn, seconds, doc)
        return fn
    return wrap


@scenario("still", 3.0, "Nothing moves: the fill is the continuation only, and must stay put")
def _still(t):
    return 0.0, 0.0, 0.0, 0.0, 0, 1.0


@scenario("pan_slow", 4.0, "The view turns 10 deg/s, the frame centered")
def _pan_slow(t):
    return 10 * t, 0.0, 10 * t, 0.0, 0, 1.0


@scenario("pan_fast", 4.0, "The view turns 90 deg/s, the frame centered")
def _pan_fast(t):
    return 90 * t, 0.0, 90 * t, 0.0, 0, 1.0


@scenario("shake", 4.0, "The view still, the frame shakes sideways (6 deg, 1.3 + 3.7 Hz)")
def _shake_s(t):
    return 0.0, 0.0, _shake(t), 0.0, 0, 1.0


@scenario("shake_pan", 4.0, "The view turns 15 deg/s, the frame shakes sideways on top")
def _shake_pan(t):
    return 15 * t, 0.0, 15 * t + _shake(t), 0.0, 0, 1.0


@scenario("shake_stop", 4.0, "The frame shakes for 2 s, then holds still in the middle")
def _shake_stop(t):
    return 0.0, 0.0, _shake(t) * (t < 2.0), 0.0, 0, 1.0


@scenario("shake_hold_side", 4.0, "The frame shakes for 2 s, then holds still 5 deg to the right")
def _shake_hold(t):
    return 0.0, 0.0, (_shake(t) if t < 2.0 else 5.0) + 5.0 * _ease((t - 1.8) / 0.4) * (t < 2.0), 0.0, 0, 1.0


@scenario("drift", 4.0, "The frame drifts slowly 8 deg to the right (the view lags behind the camera)")
def _drift(t):
    return 0.0, 0.0, 8.0 * _ease(t / 4.0), 0.0, 0, 1.0


@scenario("turn_out_back", 4.0, "The camera turns 18 deg right ahead of the view (new directions), holds, comes back")
def _turn(t):
    return 0.0, 0.0, 18.0 * (_ease((t - 0.5) / 1.0) - _ease((t - 2.5) / 1.0)), 0.0, 0, 1.0


@scenario("pan_back", 4.0, "The view turns 45 deg and back over what it saw, the frame centered")
def _pan_back(t):
    y = 45.0 * (_ease(t / 1.5) - _ease((t - 1.5) / 1.5))
    return y, 0.0, y, 0.0, 0, 1.0


@scenario("cut", 4.0, "A gentle shake; at 1.5 s the world changes (surfacing / a cut)")
def _cut(t):
    return 0.0, 0.0, _shake(t, 3.0), 0.0, int(t >= 1.5), 1.0


@scenario("cut_turn_back", 4.0, "The view turns 50 deg right, the world changes there at 1.5 s, the view comes back over the old world's traces")
def _cut_turn(t):
    y = 50.0 * (_ease(t / 1.2) - _ease((t - 2.0) / 1.5))
    return y, 0.0, y + _shake(t, 2.0), 0.0, int(t >= 1.5), 1.0


@scenario("exposure", 4.0, "A gentle shake; at 1.5 s the world gets 40 % darker (exposure, a cloud)")
def _exposure(t):
    return 0.0, 0.0, _shake(t, 3.0), 0.0, 0, 0.6 if t >= 1.5 else 1.0


@scenario("pitch_shake", 4.0, "The view still, the frame shakes up and down (4 deg)")
def _pitch(t):
    return 0.0, 0.0, 0.0, _shake(t, 4.0, 1), 0, 1.0


@scenario("roll_shake", 4.0, "The view still, the frame rolls back and forth (5 deg): its edges tilt in the window")
def _roll(t):
    return 0.0, 0.0, 0.0, 0.0, 0, 1.0, 0.0, _shake(t, 5.0, 2)


@scenario("pan_shake2d", 4.0, "The view turns 15 deg/s and drifts up; the frame shakes sideways, up-down and rolls")
def _pan2d(t):
    vy, vp = 15 * t, 4.0 * _ease(t / 4.0)
    return vy, vp, vy + _shake(t, 5.0, 0), vp + _shake(t, 3.0, 1), 0, 1.0, 0.0, _shake(t, 3.0, 2)


@scenario("pan_roll", 4.0, "The view turns 30 deg/s while the frame rolls (6 deg)")
def _pan_roll(t):
    return 30 * t, 0.0, 30 * t, 0.0, 0, 1.0, 0.0, _shake(t, 6.0, 1)


@scenario("turn_shift_combo", 4.0, "The view turns 40 deg and back while the frame swings out 12 deg right and up, rolling")
def _combo(t):
    vy = 40.0 * (_ease(t / 2.0) - _ease((t - 2.0) / 2.0))
    out = 12.0 * (_ease((t - 0.5) / 1.0) - _ease((t - 2.5) / 1.0))
    return vy, 0.0, vy + out, 0.6 * out + _shake(t, 2.0, 1), 0, 1.0, 0.0, _shake(t, 4.0, 2)


def path(name, frames=None):
    """Per frame: (view yaw, view pitch, camera yaw, camera pitch, world, gain, view roll, camera roll)"""
    fn, seconds, _ = SCENARIOS[name]
    n = frames or int(round(seconds * FPS))
    out = []
    for i in range(n):
        v = tuple(fn(i / FPS))
        out.append(v + (0.0, 0.0)[:8 - len(v)])
    return out


def cameras(p):
    """(Qv, Qc) per frame of a path"""
    return [(orient(v[0], v[1], v[6]), orient(v[2], v[3], v[7])) for v in p]


# ---- what the source camera sees ----

def source_uv(w, h):
    """The frame's pixel centers (w x h, the decoded size) in the shader's source coordinates: texel(uv) inverted"""
    i = np.arange(w)
    j = np.arange(h)
    u = (i + 0.5) * SRC_W / w - 0.5
    v = (j + 0.5) * SRC_H / h - 0.5
    return np.meshgrid(u, v)


def render_frame(w, h, qc, which, gain):
    u, v = source_uv(w, h)
    d = np.stack([(u - SRC_W / 2) / F, (v - SRC_H / 2) / F, np.ones_like(u)], -1)
    lon, lat = lonlat(qrot(qc, d))
    return world(lon, lat, which, gain)


def to_yuv420p10(rgb):
    """RGB (coded, 0..1) -> yuv420p10le bytes, BT.709 limited range (what afpchain takes)"""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    y = 0.2126 * r + 0.7152 * g + 0.0722 * b
    cb = (b - y) / 1.8556
    cr = (r - y) / 1.5748
    Y = np.clip(np.round(64 + 876 * y), 0, 1023).astype('<u2')
    def sub(c):
        c2 = 0.25 * (c[0::2, 0::2] + c[1::2, 0::2] + c[0::2, 1::2] + c[1::2, 1::2])
        return np.clip(np.round(512 + 896 * c2), 0, 1023).astype('<u2')
    return Y.tobytes() + sub(cb).tobytes() + sub(cr).tobytes()


# ---- the table ----

def table_hex(frames):
    """frames: list of (Qv, Qc). The kept shader's table layout (gpu_table.rs Table::build), one block a frame"""
    n = len(frames)
    height = (n + COLS - 1) // COLS
    data = np.zeros((height, COLS * BLOCK, 4), dtype='<f4')
    for i, (qv, qc) in enumerate(frames):
        blk = np.zeros((BLOCK, 4), dtype='<f4')
        blk[0] = (F, F, SRC_W / 2, SRC_H / 2)
        blk[7] = (FOV, 1.0, 1.0, 0.0)           # zoom, lens correction 100 %, no water, no field limit
        blk[8] = (0.0, 0.0, 1.0, 1.0)           # no 2D translation, 1 row (no rolling shutter), valid
        blk[9] = (0.0, 0.0, 0.0, n)
        blk[14] = (1.0, 1.0, 1.0, 1.0)
        blk[15] = qv
        qr = qmul(qconj(qc), qv)
        blk[PARAM_TEXELS:] = qr
        data[i // COLS, (i % COLS) * BLOCK:(i % COLS + 1) * BLOCK] = blk
    return data.tobytes().hex(), COLS * BLOCK, height


def make_shader(template_path, frames, out_path):
    """The kept shader with the synthetic table in place of the clip's"""
    lines = open(template_path).read().split('\n')
    i = lines.index('//!TEXTURE GYROFLOW_TABLE')
    assert lines[i + 1].startswith('//!SIZE') and lines[i + 2] == '//!FORMAT rgba32f'
    j = i + 3
    while lines[j].startswith('//!'):
        j += 1
    hx, w, h = table_hex(frames)
    lines[i + 1] = f'//!SIZE {w} {h}'
    lines[j] = hx
    # Only the gyroflow passes: the export's look (log -> HLG) would change the synthetic colors
    out, k = [], 0
    while k < len(lines):
        if lines[k].startswith('//!HOOK'):
            e = k + 1
            while e < len(lines) and not lines[e].startswith('//!HOOK') and not lines[e].startswith('//!TEXTURE'):
                e += 1
            block = lines[k:e]
            desc = next((l for l in block if l.startswith('//!DESC')), '')
            if desc.startswith('//!DESC gyroflow'):
                out += block
            k = e
        else:
            out.append(lines[k])
            k += 1
    # PTS comes from libplacebo's pass counter over the passes a frame (gpu_table::for_libplacebo): one fewer now
    import re
    npass = sum(1 for l in out if l.startswith('//!HOOK'))
    text = re.sub(r'\(frame - 1\) / \d+\)', f'(frame - 1) / {npass})', '\n'.join(out))
    open(out_path, 'w').write(text)
