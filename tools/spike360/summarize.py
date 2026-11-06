#!/usr/bin/env python3
"""tools/spike360/summarize.py <results> [warmup]: one row per run of run.sh in <results>, over the samples after
the first <warmup> seconds (default 3):

  fps        frames through the chain per second (playback time over wall time, times the clip's rate)
  shown      of those, per second, not dropped (vo and decoder drops taken off)
  speed      playback time over wall time (1.00: real time; with --untimed, how many times real time)
  drops      frames dropped by the vo and the decoder over the window
  late       frames the vo showed late (vo-delayed-frame-count) over the window
  cpu        mpv's CPU, in cores (top's % / 100), its mean; sys% and user% the system's
  gpu        the GPU's Device Utilization %, its mean
  mem        mpv's memory at the end (top's MEM)
  compG      the memory the system had compressed, its most, in GB (memory pressure)
  other      the busiest other process's CPU, in cores, its mean (another session's build skews a run)
"""
import csv
import pathlib
import statistics
import sys

out = pathlib.Path(sys.argv[1])
warmup = float(sys.argv[2]) if len(sys.argv) > 2 else 3.0


def num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


rows = []
for path in sorted(out.glob("*.csv")):
    data = [r for r in csv.DictReader(open(path)) if num(r["playback-time"]) is not None]
    if len(data) < 3:
        rows.append((path.stem, "no samples"))
        continue
    t0 = num(data[0]["wall"])
    win = [r for r in data if num(r["wall"]) >= t0 + warmup] or data
    a, b = win[0], win[-1]
    dt = num(b["wall"]) - num(a["wall"])
    if dt <= 0:
        rows.append((path.stem, "too short"))
        continue

    def d(key):
        return (num(b[key]) or 0) - (num(a[key]) or 0)

    rate = num(b["fps"]) or num(a["fps"]) or 0
    frames = d("playback-time") * rate
    drops = d("frame-drop-count") + d("decoder-frame-drop-count")
    late = d("vo-delayed-frame-count")
    speed = d("playback-time") / dt
    # top's samples over the same stretch (its clock starts with mpv, within a second)
    cpu = gpu = mem = sysu = syss = other = comp = None
    top = out / (path.stem + ".top")
    if top.exists():
        samples = [line.split() for line in open(top) if len(line.split()) >= 5]
        if samples:
            s0 = float(samples[0][0])
            span = num(data[-1]["wall"]) - t0
            mid = [s for s in samples if s0 + warmup <= float(s[0]) <= s0 + span - 1] or samples
            cpu = statistics.mean(float(s[3]) for s in mid) / 100
            sysu = statistics.mean(float(s[1]) for s in mid)
            syss = statistics.mean(float(s[2]) for s in mid)
            gpus = [num(s[5]) for s in mid if len(s) > 5 and num(s[5]) is not None]
            gpu = statistics.mean(gpus) if gpus else None
            comps = [num(s[6]) for s in mid if len(s) > 6 and num(s[6]) is not None]
            comp = max(comps) / 1024 if comps else None
            others = [num(s[7]) for s in mid if len(s) > 7 and num(s[7]) is not None]
            other = statistics.mean(others) / 100 if others else None
            mem = next((s[4] for s in reversed(samples) if s[4].endswith(("M", "M+", "G", "G+"))), None)
    rows.append((path.stem, f"{frames / dt:6.1f} {(frames - drops) / dt:6.1f} {speed:5.2f} {drops:5.0f} {late:5.0f}"
                 f" {cpu if cpu is not None else float('nan'):5.2f} {sysu or 0:5.1f} {syss or 0:5.1f}"
                 f" {gpu if gpu is not None else float('nan'):5.1f} {mem or '':>6} {comp if comp is not None else float('nan'):5.1f} {other if other is not None else float('nan'):5.2f}"))

print(f"{'run':28} {'fps':>6} {'shown':>6} {'speed':>5} {'drops':>5} {'late':>5} {'cpu':>5} {'user%':>5} {'sys%':>5}"
      f" {'gpu%':>5} {'mem':>6} {'compG':>5} {'other':>5}")
for name, text in rows:
    print(f"{name:28} {text}")

# Repeats (-r1, -r2...) together: the best run's fps and the median, since the Mac's other work only slows a run down
groups = {}
for name, text in rows:
    base = name.rsplit("-r", 1)[0] if "-r" in name else name
    try:
        groups.setdefault(base, []).append(float(text.split()[0]))
    except ValueError:
        pass
if any(len(v) > 1 for v in groups.values()):
    print()
    print(f"{'run (repeats)':28} {'best':>6} {'median':>6} {'n':>2}")
    for base, fps in groups.items():
        print(f"{base:28} {max(fps):6.1f} {statistics.median(fps):6.1f} {len(fps):2}")
