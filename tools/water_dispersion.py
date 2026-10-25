#!/usr/bin/env python3
"""Water's refractive index as each color channel of a camera sees it, for the gyroflow plugin's color fringes under
water (vs-gyroflow/src/dispersion.rs, WATER_RGB). Physics only, nothing fitted to footage:

  - water's index n(λ): Quan & Fry (1995), fresh water at 20 °C (salt and temperature move all colors alike);
  - the light: daylight (CIE D65), after `path` metres of pure water (Pope & Fry 1997 absorption), on a neutral grey
    object: neither the depth nor what's filmed is known, so a grey edge in daylight, with the water's path as a range;
  - the channels: what a camera's R, G, B are made to be, colorimetric RGB of its gamut (the CIE 1931 observer through
    the gamut's primaries: D-Gamut, Rec.709, Rec.2020), or a typical Bayer sensor's raw channels.

A channel's signal at an edge is the sum over wavelengths of the edge shifted by each wavelength's own refraction, so
the edge moves by the average of those shifts, weighted by how much each wavelength counts in the channel: the
channel's index is that average of n(λ). Small shifts (a few pixels) keep it linear.

  tools/water_dispersion.py            the table, and the values for WATER_RGB
"""
import numpy as np

lam = np.arange(380.0, 731.0, 1.0)  # nm


def water_index(l, salinity=0.0, temp=20.0):
    """Quan & Fry (1995), l in nm"""
    S, T = salinity, temp
    return (1.31405 + (1.779e-4 - 1.05e-6 * T + 1.6e-8 * T * T) * S - 2.02e-6 * T * T
            + (15.868 + 0.01155 * S - 0.00423 * T) / l - 4382.0 / l ** 2 + 1.1455e6 / l ** 3)


def cie1931(l):
    """The CIE 1931 2° observer: Wyman, Sloan & Shirley's multi-lobe fit (2013)"""
    def g(x, mu, s1, s2):
        return np.exp(-0.5 * ((x - mu) / np.where(x < mu, s1, s2)) ** 2)
    x = 1.056 * g(l, 599.8, 37.9, 31.0) + 0.362 * g(l, 442.0, 16.0, 26.7) - 0.065 * g(l, 501.1, 20.4, 26.2)
    y = 0.821 * g(l, 568.8, 46.9, 40.5) + 0.286 * g(l, 530.9, 16.3, 31.1)
    z = 1.217 * g(l, 437.0, 11.8, 36.0) + 0.681 * g(l, 459.0, 26.0, 13.8)
    return np.stack([x, y, z])


# CIE D65, 380-730 nm every 10
D65 = np.interp(lam, np.arange(380, 731, 10), [
    49.98, 54.65, 82.75, 91.49, 93.43, 86.68, 104.86, 117.01, 117.81, 114.86, 115.92, 108.81, 109.35, 107.80, 104.79,
    107.69, 104.41, 104.05, 100.00, 96.33, 95.79, 88.69, 90.01, 89.60, 87.70, 83.29, 83.70, 80.03, 80.21, 82.28, 78.28,
    69.72, 71.61, 74.35, 61.60, 69.89])
# Pure water's absorption, 1/m (Pope & Fry 1997, 380-720 nm; 730 extrapolated)
ABSORPTION = np.interp(lam, np.arange(380, 731, 20), [
    0.0114, 0.0066, 0.0045, 0.0064, 0.0098, 0.0150, 0.0257, 0.0477, 0.0558, 0.0708, 0.108, 0.2224, 0.2755, 0.3170,
    0.410, 0.465, 0.624, 1.231])


def gamut_rgb(primaries, white=(0.3127, 0.3290)):
    """The gamut's RGB as functions of wavelength: the observer through its primaries (linear light)"""
    def xyz(xy):
        return np.array([xy[0] / xy[1], 1.0, (1 - xy[0] - xy[1]) / xy[1]])
    P = np.stack([xyz(p) for p in primaries], 1)
    scale = np.linalg.solve(P, xyz(white))
    M = np.linalg.inv(P * scale)  # XYZ -> RGB
    return M @ cie1931(lam)


GAMUTS = {
    "D-Gamut": gamut_rgb([(0.71, 0.31), (0.21, 0.88), (0.09, -0.08)]),
    "Rec.709": gamut_rgb([(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)]),
    "Rec.2020": gamut_rgb([(0.708, 0.292), (0.170, 0.797), (0.131, 0.046)]),
}


def bayer_raw(l):
    """A typical small CMOS sensor's raw R, G, B (color filters × silicon), behind an IR cut filter at ~650 nm"""
    def g(mu, s):
        return np.exp(-0.5 * ((l - mu) / s) ** 2)
    ir_cut = 1.0 / (1.0 + np.exp((l - 655.0) / 9.0))
    r = (g(600, 32) + 0.08 * g(530, 30) + 0.05 * g(450, 25)) * ir_cut
    gr = g(535, 42) + 0.10 * g(610, 30)
    b = g(460, 32) + 0.12 * g(530, 30)
    return np.stack([r, gr, b]) * ir_cut


def channel_index(sensitivity, path):
    light = D65 * np.exp(-ABSORPTION * path)
    n = water_index(lam)
    w = sensitivity * light
    return (w * n).sum(1) / w.sum(1)


if __name__ == "__main__":
    paths = (0.0, 1.0, 3.0, 5.0, 10.0)
    print("water's index per channel, and red's and blue's difference from green (×1e-4), by the light's path in water")
    print(f"{'channels':12s} " + "  ".join(f"{p:>4.0f} m: R-G   B-G" for p in paths))
    rows = dict(GAMUTS)
    rows["Bayer raw"] = bayer_raw(lam)
    for name, s in rows.items():
        cells = []
        for p in paths:
            n = channel_index(s, p)
            cells.append(f"      {(n[0] - n[1]) * 1e4:+5.1f} {(n[2] - n[1]) * 1e4:+5.1f}")
        print(f"{name:12s} " + "  ".join(cells))
    n = channel_index(GAMUTS["D-Gamut"], 3.0)
    print("\nWATER_RGB (D-Gamut, 3 m):", "[" + ", ".join(f"{v:.5f}" for v in n) + "]")
