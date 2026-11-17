# cmp.py A B W H frames... : per-plane PSNR (10-bit), mean abs, max abs, p99.9 abs, for the listed frames of two videos
import sys, subprocess, numpy as np
a, b, W, H = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
frames = [int(x) for x in sys.argv[5:]]
def grab(path, n):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", f"select=eq(n\\,{n})", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"], capture_output=True).stdout
    v = np.frombuffer(raw, np.uint16).astype(np.int32)
    y = v[:W*H].reshape(H, W); u = v[W*H:W*H*5//4].reshape(H//2, W//2); w = v[W*H*5//4:].reshape(H//2, W//2)
    return y, u, w
tot = {k: [] for k in "YUV"}
for n in frames:
    pa, pb = grab(a, n), grab(b, n)
    out = [f"frame {n:4d}"]
    for k, x, y in zip("YUV", pa, pb):
        d = np.abs(x - y); mse = (d.astype(float) ** 2).mean()
        psnr = 99.0 if mse == 0 else 10 * np.log10(1023 ** 2 / mse)
        tot[k].append(mse)
        out.append(f"{k}: {psnr:5.1f} dB mean {d.mean():5.2f} p99.9 {np.percentile(d, 99.9):4.0f} max {d.max():4d}")
    print("  ".join(out))
print("all   " + "  ".join(f"{k}: {10*np.log10(1023**2/np.mean(v)):5.1f} dB" for k, v in tot.items()))
