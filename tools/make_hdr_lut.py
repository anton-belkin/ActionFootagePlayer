#!/usr/bin/env python3
"""Make a log -> Rec.2100 HLG .cube LUT from a log curve's published formulas, like luts/dlogm_hlg.cube is for D-Log M.

    make_hdr_lut.py <curve> <out.cube> [--size N]
    make_hdr_lut.py --from-dwgi <in.cube> <out.cube> [--size N] [--note <line>]...

Curves, each from its maker's own publication (docs/luts.md has the sources):
  dlog    DJI D-Log / D-Gamut: DJI's ACES input transform, Dlog_DGamut_to_ACES_AP0_cat02_v1, in DJI's "ACES Workflow
          Guide" (2023-12). Mavic 3 family, Mavic 4 Pro, Osmo Pocket 4/4P in D-Log.
  gplog2  GoPro GP-Log2: log base 600, Rec.2020 primaries, GoPro's default +1.8 EV, from GoPro Labs' GP-Log2 LUT
          Generator (https://gopro.github.io/labs/gplog2/, github.com/gopro/labs, Apache License 2.0; the formulas
          are reimplemented here, no code is copied). MISSION 1, MISSION 1 PRO.
  ilog    Insta360 10-bit I-Log, BT.2020 primaries: the Insta360 10-bit I-Log White Paper (v1.0, June 2026). X6, Luna
          Ultra, Luna Pro; not the 8-bit I-Log of older models.
--from-dwgi takes a LUT whose output is DaVinci Wide Gamut / DaVinci Intermediate (Blackmagic Design's published
spaces), such as Zeb Gardner's GP-Log -> DWG/I, and its comment lines (its author) go into the output's header, with
each --note (where it comes from, its terms) after them.

All of them render the same way: scene light, with 18 % grey at 0.18, is scaled by 0.254 and rolled off by
x / (1 + (x/0.69)^1.5)^(1/1.5) per channel, then HLG-encoded (ITU-R BT.2100) in Rec.2020. Those numbers are fitted
to luts/dlogm_hlg.cube (from Zeb Gardner's D-Log M -> DWG/I), so every curve looks like D-Log M does: 18 % grey
at about 37 % HLG (BT.2408 puts it at 38 %), highlights below HLG's peak. Plain Python, no numpy: install.sh runs it.
"""
import math
import os
import sys

# The HLG rendering, fitted to luts/dlogm_hlg.cube
EXPOSURE = 0.254
SHOULDER = 0.69
SHOULDER_POWER = 1.5

D65 = (0.3127, 0.3290)
REC2020 = ((0.708, 0.292), (0.170, 0.797), (0.131, 0.046))


def rgb_to_xyz(primaries, white):
    """The RGB -> XYZ matrix of a set of primaries and white point, rows of 3."""
    cols = [(x / y, 1.0, (1 - x - y) / y) for x, y in primaries]
    m = [[cols[j][i] for j in range(3)] for i in range(3)]
    wx, wy = white
    s = solve(m, (wx / wy, 1.0, (1 - wx - wy) / wy))
    return [[m[i][j] * s[j] for j in range(3)] for i in range(3)]


def inverse(m):
    a, b, c = m[0]
    d, e, f = m[1]
    g, h, i = m[2]
    det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    return [[(e * i - f * h) / det, (c * h - b * i) / det, (b * f - c * e) / det],
            [(f * g - d * i) / det, (a * i - c * g) / det, (c * d - a * f) / det],
            [(d * h - e * g) / det, (b * g - a * h) / det, (a * e - b * d) / det]]


def solve(m, v):
    return mul(inverse(m), v)


def mul(m, v):
    return tuple(m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] for i in range(3))


def matmul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def to_rec2020(primaries):
    """From a D65 RGB space to Rec.2020 (also D65: no white point adaptation)."""
    return matmul(inverse(rgb_to_xyz(REC2020, D65)), rgb_to_xyz(primaries, D65))


# DJI D-Log / D-Gamut: DJI's Dlog_DGamut_to_ACES_AP0_cat02_v1 (its dlog_to_lin, and DGamut_PRI)
def dlog(x):
    if x <= 0.14:
        return (x - 0.0929) / 6.025
    return (10 ** (3.89616 * x - 2.27752) - 0.0108) / 0.9892


D_GAMUT = ((0.71, 0.31), (0.21, 0.88), (0.09, -0.08))


# GoPro GP-Log2: GoPro Labs' invLog (log base 600, 1.0 = clip), times its default EV gain 2^1.8, which puts the
# scene's 18 % grey at 0.18 (GoPro: "L = 0.0517 (18% grey)")
def gplog2(x):
    return 2 ** 1.8 * (600 ** x - 1) / 599


# Insta360 10-bit I-Log: the white paper's "I-Log to Scene-Linear Reflection"
def ilog(x):
    if x < 0.154402:
        return (x - 0.09055934) / 5.77837328
    return 10 ** ((x - 0.623992) / 0.280055) - 0.01


# DaVinci Intermediate -> linear, and DaVinci Wide Gamut, as Blackmagic Design publishes them
def davinci_intermediate(y):
    if y > 0.02740668:
        return 2 ** (y / 0.07329248 - 7.0) - 0.0075
    return y / 10.44426855


DWG = ((0.8000, 0.3130), (0.1682, 0.9877), (0.0790, -0.1155))

CURVES = {
    'dlog': (dlog, D_GAMUT, [
        'DJI D-Log / D-Gamut -> Rec.2100 HLG (Rec.2020, 1000-nit reference).',
        "D-Log and D-Gamut from DJI's ACES input transform Dlog_DGamut_to_ACES_AP0_cat02_v1, in DJI's",
        '"ACES Workflow Guide" (2023-12), https://terra-1-g.djicdn.com/851d20f7b9f64838a34cd02351370894/ACES/'
        'ACES%20Workflow%20Guide%20.zip',
    ]),
    'gplog2': (gplog2, REC2020, [
        'GoPro GP-Log2 -> Rec.2100 HLG (Rec.2020, 1000-nit reference).',
        "GP-Log2 (log base 600, Rec.2020 primaries, +1.8 EV) from GoPro Labs' GP-Log2 LUT Generator,",
        'https://gopro.github.io/labs/gplog2/ (github.com/gopro/labs, Copyright GoPro, Inc., Apache License 2.0;',
        'the formulas reimplemented, no code copied).',
    ]),
    'ilog': (ilog, REC2020, [
        'Insta360 10-bit I-Log -> Rec.2100 HLG (Rec.2020, 1000-nit reference).',
        'I-Log and its BT.2020 gamut from the Insta360 10-bit I-Log White Paper (v1.0, June 2026),',
        'https://wassets.insta360.com/common/31f0330509384b7b8ba0a07f4ff4eb13/Insta360_10-bit_I-Log_White_Paper.pdf',
    ]),
}

RENDERING = [
    'Rendering: scene light x0.254, roll-off x/(1+(x/0.69)^1.5)^(1/1.5), fitted to luts/dlogm_hlg.cube (from',
    "Zeb Gardner's D-Log M -> DWG/I, https://www.zebgardner.com/); HLG per ITU-R BT.2100.",
    'Made by tools/make_hdr_lut.py (ActionFootagePlayer, GPL-3.0).',
]


def hlg(e):
    """BT.2100 HLG OETF."""
    if e <= 1 / 12:
        return math.sqrt(3 * e)
    return 0.17883277 * math.log(12 * e - 0.28466892) + 0.55991073


def render(rgb):
    """Scene-linear Rec.2020 (18 % grey = 0.18) -> HLG signal."""
    out = []
    for v in rgb:
        x = max(0.0, v) * EXPOSURE
        x = x / (1 + (x / SHOULDER) ** SHOULDER_POWER) ** (1 / SHOULDER_POWER)
        out.append(hlg(x))
    return out


def read_cube(path):
    """A .cube with an optional 1D shaper before its 3D LUT (as Resolve writes them): (shaper, 3D LUT, comments)."""
    size1 = size3 = None
    range1 = range3 = (0.0, 1.0)
    values, comments = [], []
    with open(path, errors='replace') as f:
        for line in f:
            t = line.split()
            if not t:
                continue
            if t[0].startswith('#'):
                comments.append(line.strip().lstrip('#').strip())
            elif t[0] == 'LUT_1D_SIZE':
                size1 = int(t[1])
            elif t[0] == 'LUT_3D_SIZE':
                size3 = int(t[1])
            elif t[0] == 'LUT_1D_INPUT_RANGE':
                range1 = (float(t[1]), float(t[2]))
            elif t[0] == 'LUT_3D_INPUT_RANGE':
                range3 = (float(t[1]), float(t[2]))
            elif t[0] == 'TITLE':
                comments.insert(0, line.split(None, 1)[1].strip().strip('"'))
            elif t[0][0].isalpha():
                continue
            else:
                values.append(tuple(float(x) for x in t[:3]))
    n1 = size1 or 0
    if size3 is None or len(values) != n1 + size3 ** 3:
        sys.exit(f'{path}: not a 3D .cube LUT')
    return (values[:n1], range1), (values[n1:], size3, range3), comments


def apply_cube(cube, rgb):
    (shaper, range1), (lut, n, range3), _ = cube
    if shaper:
        m = len(shaper) - 1
        out = []
        for c, v in enumerate(rgb):
            p = min(max((v - range1[0]) / (range1[1] - range1[0]), 0.0), 1.0) * m
            i = min(int(p), m - 1)
            out.append(shaper[i][c] + (shaper[i + 1][c] - shaper[i][c]) * (p - i))
        rgb = out
    p = [min(max((v - range3[0]) / (range3[1] - range3[0]), 0.0), 1.0) * (n - 1) for v in rgb]
    i = [min(int(x), n - 2) for x in p]
    f = [x - j for x, j in zip(p, i)]
    res = [0.0, 0.0, 0.0]
    for dr in (0, 1):
        for dg in (0, 1):
            for db in (0, 1):
                w = (f[0] if dr else 1 - f[0]) * (f[1] if dg else 1 - f[1]) * (f[2] if db else 1 - f[2])
                if w:
                    e = lut[(i[0] + dr) + (i[1] + dg) * n + (i[2] + db) * n * n]  # red fastest
                    for c in range(3):
                        res[c] += w * e[c]
    return res


def write_cube(path, size, header, transform):
    with open(path, 'w') as f:
        for line in header:
            f.write(f'# {line}\n' if line else '#\n')
        f.write(f'LUT_3D_SIZE {size}\n\n')
        steps = [k / (size - 1) for k in range(size)]
        for b in steps:
            for g in steps:
                for r in steps:
                    f.write('%.6f %.6f %.6f\n' % tuple(transform((r, g, b))))


def main():
    args = sys.argv[1:]
    size, notes = 65, []
    if '--size' in args:
        k = args.index('--size')
        size = int(args[k + 1])
        del args[k:k + 2]
    while '--note' in args:
        k = args.index('--note')
        notes.append(args[k + 1])
        del args[k:k + 2]
    if len(args) == 3 and args[0] == '--from-dwgi':
        cube = read_cube(args[1])
        m = to_rec2020(DWG)
        header = ['Rec.2100 HLG (Rec.2020, 1000-nit reference), from a LUT to DaVinci Wide Gamut/Intermediate,',
                  f'{os.path.basename(args[1])}, which says:', *cube[2], *notes, '']

        def transform(rgb):
            return render(mul(m, [davinci_intermediate(v) for v in apply_cube(cube, rgb)]))
    elif len(args) == 2 and args[0] in CURVES:
        decode, primaries, header = CURVES[args[0]]
        m = to_rec2020(primaries)
        header = [*header, '']

        def transform(rgb):
            return render(mul(m, [decode(v) for v in rgb]))
    else:
        sys.exit(__doc__)
    out = args[-1]
    write_cube(out, size, header + RENDERING, transform)
    print(f'{out}: {size}³ LUT')


if __name__ == '__main__':
    main()
