#!/usr/bin/env python3
# tools/latency360/summarize.py <mpv log> [...]: the time from input to the screen, from the app's mpv run with
# --macos-latency-log (tools/mpv-patches/0010) and --log-file (or --msg-level=latency=info on the terminal).
#
# Each frame mpv shows logs, in seconds of the host clock (NSEvent.timestamp's, a drawable's presentedTime's):
#   applied   the input event whose effect the frame shows (what the script turning the view said, in glsl-shader-opts
#             afp_input_time; 0: none)
#   newest    the newest input event when the frame started being drawn
#   begin     the VO started drawing it
#   drawable  MoltenVK took the Metal layer's drawable for it
#   submit    it was handed to the swapchain
#   shown     the drawable was shown (presentedTime: the refresh it went out in; 0: dropped)
# For each input, the first frame that shows it counts: input -> light is shown - applied, in the stages
#   input -> begin (the gesture through the scripts, waiting for the VO to draw), begin -> submit (rendering),
#   submit -> shown (the swapchain's queue and the wait for the refresh).
# What's not in it: the trackpad's own scan and the HID path before the event's time (a 240 fps camera sees it), and
# the scan-out down the screen after presentedTime (half a refresh to the middle).
import re
import statistics
import sys

LINE = re.compile(r"frame=(\d+) applied=([\d.]+) newest=([\d.]+) begin=([\d.]+) drawable=([\d.]+) "
                  r"submit=([\d.]+) shown=([\d.]+)(?: fresh=(\d))?")


def pct(values, p):
    v = sorted(values)
    if not v:
        return float("nan")
    k = (len(v) - 1) * p / 100
    lo = int(k)
    hi = min(lo + 1, len(v) - 1)
    return v[lo] + (v[hi] - v[lo]) * (k - lo)


def row(name, values):
    ms = [x * 1000 for x in values]
    if not ms:
        return f"  {name:<22} (none)"
    return (f"  {name:<22} n={len(ms):<5} p50 {pct(ms, 50):6.1f}  p90 {pct(ms, 90):6.1f}  p99 {pct(ms, 99):6.1f}  "
            f"max {max(ms):6.1f}  mean {statistics.fmean(ms):6.1f} ms")


def summarize(path):
    frames = []
    for line in open(path, errors="replace"):
        m = LINE.search(line)
        if m:
            f = dict(zip(("frame", "applied", "newest", "begin", "drawable", "submit", "shown"),
                         (int(m[1]),) + tuple(float(x) for x in m.groups()[1:7])))
            # Rendered from the video frame, not the renderer's cache (patch 0011 logs it; before it, taken as yes)
            f["fresh"] = m[8] != "0"
            frames.append(f)
    print(f"{path}: {len(frames)} frames")
    if not frames:
        return
    shown = [f for f in frames if f["shown"] > 0]
    dropped = len(frames) - len(shown)
    gaps = [b["shown"] - a["shown"] for a, b in zip(shown, shown[1:]) if b["shown"] > a["shown"]]
    refresh = statistics.median(gaps) if gaps else float("nan")
    span = shown[-1]["shown"] - shown[0]["shown"] if len(shown) > 1 else 0
    print(f"  refresh {refresh * 1000:.2f} ms ({1 / refresh:.1f} Hz), {len(shown) / span if span else 0:.1f} frames/s "
          f"shown, {dropped} not shown")
    bad = [f for f in shown if f["applied"] > f["shown"] or f["begin"] > f["shown"]]
    if bad:
        print(f"  !! {len(bad)} frames shown before their input or their drawing: the clocks don't match")

    # The first frame showing each new input: rendered fresh (a frame from the renderer's cache shows the parameters
    # it was rendered with, whatever the frame's own)
    first, last, stale = [], 0.0, 0
    for f in shown:
        if f["applied"] > last:
            if not f["fresh"]:
                stale += 1
                continue
            first.append(f)
            last = f["applied"]
    print(f"  inputs shown: {len(first)}, frames with new input from the cache: {stale}, "
          f"fresh frames {sum(f['fresh'] for f in shown)} of {len(shown)}")
    print(row("input -> light", [f["shown"] - f["applied"] for f in first]))
    print(row("  input -> begin", [f["begin"] - f["applied"] for f in first]))
    print(row("  begin -> submit", [f["submit"] - f["begin"] for f in first if f["submit"]]))
    print(row("  submit -> light", [f["shown"] - f["submit"] for f in first if f["submit"]]))
    print(row("  begin -> drawable", [f["drawable"] - f["begin"] for f in first if f["drawable"]]))
    # The newest input the VO could have drawn, had it taken it itself when it began
    print(row("newest@begin -> light", [f["shown"] - f["newest"] for f in shown if f["newest"] > 0
                                        and f["shown"] - f["newest"] < 1]))
    # Frames that showed nothing new while inputs were coming (a redraw of the same params, or none at all)
    inputs = sorted({f["newest"] for f in frames if f["newest"] > 0})
    if inputs:
        print(f"  input events seen: {len(inputs)} over {inputs[-1] - inputs[0]:.1f} s")


for p in sys.argv[1:]:
    summarize(p)
