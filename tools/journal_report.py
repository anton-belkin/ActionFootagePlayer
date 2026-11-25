#!/usr/bin/env python3
"""Where the waiting is: reads the responsiveness journal (docs/design/responsiveness-journal.md) over a period and says, per
action, how often it was done, how long its result took to show (median, p90, worst), the time lost in all (each
time's wait, added up), what the slow ones spent their time on, and what the slow ones had in common (a NAS, a big
file, the first open of a run, a file not opened before). Ranked by the time lost, for a person; and as JSON, every
action with its steps, for a later look.

    tools/journal_report.py                       # the last 7 days, from ~/Library/Logs/ActionFootagePlayer/journal
    tools/journal_report.py --since 2026-11-19 --json report.json
    tools/journal_report.py --dir <folder> --since 30d --top 30 --action gyroflow/still

Plain Python 3, no packages. Reads only; the journal never leaves this computer.
"""

import argparse
import datetime as dt
import json
import os
import statistics
import sys
from collections import defaultdict

# A result within this is instant: what's beyond it is "waiting" (the second total)
INSTANT_MS = 100
# Slow: worse than this many ms and than the action's own median (the cases whose steps are looked into)
SLOW_MS = 300
# A show not from the action's own code (the last action asked when another plugin drew) counts this soon after it
WEAK_MS = 500


def default_dir():
    d = os.environ.get("AFP_JOURNAL_DIR")
    if d:
        return d
    home = os.path.expanduser("~")
    if sys.platform == "darwin":
        return os.path.join(home, "Library/Logs/ActionFootagePlayer/journal")
    if os.name == "nt":
        return os.path.join(os.environ.get("LOCALAPPDATA", home), "ActionFootagePlayer", "Logs", "journal")
    return os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.join(home, ".local/state"), "ActionFootagePlayer/journal")


def parse_when(s, now):
    """'7d', '12h', '2026-11-19', '2026-11-19T10:00' -> Unix ms"""
    if s is None:
        return None
    s = s.strip()
    if s[-1:] in "dhm" and s[:-1].replace(".", "").isdigit():
        n = float(s[:-1])
        sec = {"d": 86400, "h": 3600, "m": 60}[s[-1]] * n
        return now - sec * 1000
    return dt.datetime.fromisoformat(s).timestamp() * 1000


def load(folder, since, until):
    """Every record of the files in the folder in the period, each with "file" (its process) set"""
    runs = []
    for name in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        if not name.endswith(".jsonl"):
            continue
        path = os.path.join(folder, name)
        if since and os.path.getmtime(path) * 1000 < since:
            continue
        recs = []
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(r, dict) or not isinstance(r.get("w"), (int, float)):
                    continue
                if since and r["w"] < since or until and r["w"] > until:
                    continue
                r["file"] = name
                recs.append(r)
        if recs:
            runs.append((name, recs))
    return runs


def pct(xs, p):
    xs = sorted(xs)
    if not xs:
        return None
    k = (len(xs) - 1) * p
    lo, hi = int(k), min(int(k) + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def fmt_ms(ms):
    if ms is None:
        return "—"
    if ms >= 60000:
        return f"{ms / 60000:.1f} min"
    if ms >= 1000:
        return f"{ms / 1000:.2f} s"
    return f"{ms:.0f} ms"


def build(runs):
    """The actions, each: id, name, app, file, w (asked), response (first thing on screen), result (its result on
    screen), steps (name, w, d), context"""
    actions = {}
    by_file = {}
    parents = {}         # Library action id -> the player's action it opened (handoff)
    batches = []
    costs = []
    for name, recs in runs:
        app = "library" if name.startswith("library-") else ("batch" if name.startswith("batch-") else "player")
        run = next((r for r in recs if r.get("k") == "run"), {})
        by_file[name] = {"app": app, "run": run, "recs": recs}
        if app == "batch":
            ws = [r["w"] for r in recs]
            batches.append({"file": name, "parent": run.get("parent"), "start": run.get("start") or min(ws), "end": max(ws)})
        for r in recs:
            k = r.get("k")
            if k == "cost":
                costs.append(dict(r, app=app))
            elif k == "act":
                a = r.get("a")
                if not a:
                    continue
                x = actions.get(a)
                if x is None:
                    x = actions[a] = {"id": a, "name": r.get("n"), "app": app, "file": name, "w": r["w"], "shows": {},
                                      "steps": [], "ctx": {}, "parts": {}, "fields": {}}
                x["w"] = min(x["w"], r["w"])
                for f in ("via", "key", "v", "delay", "d", "parent", "pos", "batch"):
                    if f in r:
                        x["fields"][f] = r[f]
        for r in recs:
            k, a = r.get("k"), r.get("a")
            x = actions.get(a) if a else None
            if k == "show" and x is not None:
                n = r.get("n")
                if n not in x["shows"] or r["w"] < x["shows"][n]:
                    x["shows"][n] = r["w"]
                    if r.get("weak"):
                        x.setdefault("weak", set()).add(n)
                    else:
                        x.get("weak", set()).discard(n)
            elif k == "step":
                if r.get("n") == "handoff" and r.get("parent") and app != "library" and a:
                    parents[r["parent"]] = a
                if x is not None:
                    x["steps"].append({"n": r.get("n"), "w": r["w"], "d": r.get("d"), "s": r.get("s"),
                                       **{f: r[f] for f in ("where", "size", "nth", "again", "since_ms", "status", "op", "path", "items", "view", "reused", "made") if f in r}})
                    for f in ("where", "size", "nth", "again", "moment", "w_px", "h_px", "fps", "codec", "hwdec", "duration"):
                        if f in r and r[f] is not None:
                            x["ctx"].setdefault(f, r[f])
                    if r.get("n") in ("smooth", "not-smooth"):
                        x["smooth"] = r["w"] - x["w"] if r.get("n") == "smooth" else None
                        x["ctx"]["drops"] = r.get("drops")
            elif k == "io" and x is not None:
                x["ctx"]["nas_ms"] = x["ctx"].get("nas_ms", 0) + (r.get("nas_ms") or 0)
                x["ctx"]["nas_ops"] = x["ctx"].get("nas_ops", 0) + (r.get("nas_n") or 0)
                x["ctx"]["local_io_ms"] = x["ctx"].get("local_io_ms", 0) + (r.get("local_ms") or 0)
            elif k == "done" and a:
                if x is None:
                    x = actions[a] = {"id": a, "name": r.get("n"), "app": app, "file": name, "w": r["w"], "shows": {},
                                      "steps": [], "ctx": {}, "parts": {}, "fields": {}}
                x["done"] = r
                x["parts"] = r.get("parts") or {}
                if r.get("images"):
                    x["ctx"]["images"] = r["images"]
        # The app's own steps (Rust: no action id) belong to the page's action they fall in
        if app == "library":
            pages = sorted((x for x in actions.values() if x["file"] == name and x.get("done")), key=lambda x: x["w"])
            for r in recs:
                if r.get("k") != "step" or r.get("s") != "app" or r.get("a"):
                    continue
                end = r["w"] + (r.get("d") or 0)
                owner = None
                for x in pages:
                    settled = x["done"].get("settled") or x["done"].get("paint") or x["w"]
                    if x["w"] <= r["w"] and end <= settled + 50:
                        owner = x
                if owner:
                    owner["steps"].append({"n": "app:" + str(r.get("n")), "w": r["w"], "d": r.get("d"),
                                           **{f: r[f] for f in ("where", "items", "view", "made") if f in r}})
                    if r.get("where") == "nas":
                        owner["ctx"]["where"] = "nas"
                    elif "where" in r:
                        owner["ctx"].setdefault("where", r["where"])
    # Results
    for x in actions.values():
        shows = x["shows"]
        if x["app"] == "library" and x.get("done"):
            d = x["done"]
            x["response"] = d.get("paint")
            x["result"] = d.get("settled") or d.get("paint")
            if d.get("last_image"):
                x["pictures"] = d["last_image"] - x["w"]
        elif x["name"] in ("player/start", "file/open", "seek") or "frame" in shows:
            x["response"] = min(shows.values()) if shows else None
            x["result"] = shows.get("frame") or (max(shows.values()) if shows else None)
            # A clip opening on the GPU path: its first frame is the black hold; the result is the stabilized picture
            if x["name"] in ("player/start", "file/open") and shows.get("picture") and x["result"]:
                x["result"] = max(x["result"], shows["picture"])
        else:
            # A show from the action's own code counts whenever it came; one only near it in time (another plugin
            # drawing while it was the last action) counts within WEAK_MS of it
            weak = x.get("weak", set())
            own = [w for n, w in shows.items() if n not in weak or w - x["w"] <= WEAK_MS]
            x["response"] = min(shows.values()) if shows else None
            x["result"] = max(own) if own else None
        # Opened in the player from the Library: its first frame is this action's result
        p = parents.get(x["id"])
        if p and p in actions:
            px = actions[p]
            # (counted here, not again as the player's own file/open)
            px["opened_by"] = x["id"]
            frame = px["shows"].get("frame")
            x["player"] = p
            # Its steps in the player, from the click
            x["steps"] = x["steps"] + [dict(st, n="player:" + str(st["n"])) for st in px["steps"]]
            if frame:
                x["player_frame"] = frame - x["w"]
                x["result"] = max(x.get("result") or 0, frame)
                x["ctx"].update({k: v for k, v in px["ctx"].items() if k in ("where", "size", "again", "moment")})
            x["name"] = x["name"] + " → player"
        x["response_ms"] = x["response"] - x["w"] if x.get("response") else None
        x["result_ms"] = x["result"] - x["w"] if x.get("result") else None
    # First of its kind in its run (cold) or not
    seen = set()
    for x in sorted(actions.values(), key=lambda x: x["w"]):
        key = (x["file"], x["name"])
        x["ctx"]["first_in_run"] = key not in seen
        seen.add(key)
    return actions, costs, batches, by_file


def breakdown(slow):
    """Per step name: in how many of the slow cases, the mean ms, the share of their time"""
    tot = defaultdict(float)
    n = defaultdict(int)
    whole = sum(x["result_ms"] for x in slow) or 1
    for x in slow:
        names = set()
        points = set()
        for s in x["steps"]:
            if s["n"] in x["parts"]:
                continue  # the page's: in its parts already (a slow one is also a record of its own)
            if isinstance(s.get("d"), (int, float)):
                tot[s["n"]] += s["d"]
                names.add(s["n"])
            elif s["n"].split(":")[-1] in ("file-loaded", "first-frame", "video-reconfig", "start-file", "warp-set", "vf-set", "handoff", "page-start", "dom-ready") and s["n"] not in points:
                # A point: when it first came, from the action's start
                points.add(s["n"])
                tot["@" + s["n"]] += s["w"] - x["w"]
                names.add("@" + s["n"])
        for name, p in x["parts"].items():
            tot[name] += p[1]
            names.add(name)
        if x.get("player_frame") is not None:
            tot["player: click → first frame"] += x["player_frame"]
            names.add("player: click → first frame")
        for name in names:
            n[name] += 1
    rows = []
    for name in tot:
        mean = tot[name] / max(n[name], 1)
        rows.append({"step": name, "cases": n[name], "mean_ms": round(mean, 1), "share": round(tot[name] / whole, 3) if not name.startswith("@") else None})
    rows.sort(key=lambda r: -(r["mean_ms"] if r["step"].startswith("@") else r["mean_ms"] * r["cases"]))
    return rows


def correlate(xs):
    """What the slow ones have in common: the median result with and without each trait"""
    traits = {
        "on the NAS": lambda x: x["ctx"].get("where") == "nas" or (x["ctx"].get("nas_ops") or 0) > 0,
        "first of its kind in the run": lambda x: x["ctx"].get("first_in_run"),
        "file opened before in the run (warm)": lambda x: bool(x["ctx"].get("again")),
        "a moment": lambda x: bool(x["ctx"].get("moment")),
        "file > 1 GB": lambda x: (x["ctx"].get("size") or 0) > 1e9,
        "pictures loaded": lambda x: (x["ctx"].get("images") or 0) > 0,
    }
    out = []
    for name, f in traits.items():
        yes = [x["result_ms"] for x in xs if f(x)]
        no = [x["result_ms"] for x in xs if not f(x)]
        if len(yes) >= 1 and len(no) >= 1 and len(yes) + len(no) >= 3:
            my, mn = statistics.median(yes), statistics.median(no)
            out.append({"trait": name, "with": round(my, 1), "n_with": len(yes), "without": round(mn, 1), "n_without": len(no),
                        "ratio": round(my / mn, 2) if mn > 0 else None})
    out.sort(key=lambda r: -(r["ratio"] or 0))
    return out


def report(actions, costs, batches, by_file, top, only=None):
    groups = defaultdict(list)
    no_result = defaultdict(int)
    for x in actions.values():
        if only and x["name"] != only or x.get("opened_by"):
            continue
        if x.get("result_ms") is None:
            no_result[x["name"]] += 1
            continue
        groups[x["name"]].append(x)
    rows = []
    for name, xs in groups.items():
        r = [x["result_ms"] for x in xs]
        resp = [x["response_ms"] for x in xs if x.get("response_ms") is not None]
        med = statistics.median(r)
        slow = [x for x in xs if x["result_ms"] >= max(SLOW_MS, med)] or sorted(xs, key=lambda x: -x["result_ms"])[:1]
        rows.append({
            "action": name, "app": xs[0]["app"], "count": len(xs), "no_result": no_result.get(name, 0),
            "median_ms": round(med, 1), "p90_ms": round(pct(r, 0.9), 1), "worst_ms": round(max(r), 1),
            "response_median_ms": round(statistics.median(resp), 1) if resp else None,
            "lost_ms": round(sum(r), 1), "waiting_ms": round(sum(max(0, v - INSTANT_MS) for v in r), 1),
            "smooth_median_ms": round(statistics.median([x["smooth"] for x in xs if x.get("smooth") is not None]), 1)
                if any(x.get("smooth") is not None for x in xs) else None,
            "pictures_median_ms": round(statistics.median([x["pictures"] for x in xs if x.get("pictures")]), 1)
                if any(x.get("pictures") for x in xs) else None,
            "slow_cases": len(slow),
            "breakdown": breakdown(slow)[:12],
            "context": correlate(xs),
            "worst": sorted(({"id": x["id"], "file": x["file"], "result_ms": round(x["result_ms"], 1), "ctx": x["ctx"], "fields": x["fields"]}
                             for x in xs), key=lambda y: -y["result_ms"])[:5],
        })
    rows.sort(key=lambda r: -r["waiting_ms"])
    for name, n in no_result.items():
        if name not in groups:
            rows.append({"action": name, "count": n, "no_result": n, "waiting_ms": 0, "lost_ms": 0})

    # The journal's own cost: the last cost record of each process and plugin
    last = {}
    for c in costs:
        last[(c["file"], c.get("s"))] = c
    overhead = {"records": sum(c.get("records", 0) for c in last.values()),
                "record_ms": round(sum(c.get("rec_ms", 0) or 0 for c in last.values()), 1),
                "flush_ms": round(sum(c.get("flush_ms", 0) or 0 for c in last.values()), 1),
                "flush_max_ms": round(max([c.get("flush_max_ms", 0) or 0 for c in last.values()] or [0]), 2),
                "bytes": sum(c.get("bytes", 0) or 0 for c in last.values() if c.get("s") != "page")}
    return rows, overhead


def text(rows, overhead, actions, by_file, batches, top, period):
    out = []
    n_runs = defaultdict(int)
    for f in by_file.values():
        n_runs[f["app"]] += 1
    out.append(f"Responsiveness journal, {period}: {sum(1 for _ in actions)} actions, "
               + ", ".join(f"{n} {k} run{'s' if n != 1 else ''}" for k, n in sorted(n_runs.items())))
    out.append(f"Ranked by the waiting (each time's result past {INSTANT_MS} ms, added up). Times: asked → result on screen.")
    out.append("")
    out.append(f"{'#':>2}  {'action':<36} {'times':>5} {'median':>9} {'p90':>9} {'worst':>9} {'waiting':>9}  first thing shown")
    for i, r in enumerate([r for r in rows if r.get("median_ms") is not None][:top], 1):
        out.append(f"{i:>2}  {r['action'][:36]:<36} {r['count']:>5} {fmt_ms(r['median_ms']):>9} {fmt_ms(r['p90_ms']):>9} "
                   f"{fmt_ms(r['worst_ms']):>9} {fmt_ms(r['waiting_ms']):>9}  {fmt_ms(r['response_median_ms'])}")
    missing = [r for r in rows if r.get("no_result")]
    if missing:
        out.append("")
        out.append("No result seen on screen (nothing drawn for them was journaled): "
                   + ", ".join(f"{r['action']} ×{r['no_result']}" for r in missing[:12]))
    out.append("")
    for r in [r for r in rows if r.get("median_ms") is not None][:top]:
        if r["waiting_ms"] <= 0:
            continue
        out.append(f"── {r['action']}  ({r['count']}×, median {fmt_ms(r['median_ms'])}, worst {fmt_ms(r['worst_ms'])})")
        extra = []
        if r.get("smooth_median_ms") is not None:
            extra.append(f"playing without dropped frames after {fmt_ms(r['smooth_median_ms'])} (median)")
        if r.get("pictures_median_ms") is not None:
            extra.append(f"last picture in after {fmt_ms(r['pictures_median_ms'])} (median)")
        if extra:
            out.append("   " + "; ".join(extra))
        out.append(f"   In the {r['slow_cases']} slow one(s), the time went to (steps can overlap: % of the wait each took):")
        for b in r["breakdown"][:8]:
            if b["step"].startswith("@"):
                out.append(f"     {b['step'][1:]:<34} came at {fmt_ms(b['mean_ms'])} (mean, {b['cases']} case(s))")
            else:
                share = f"{b['share'] * 100:.0f}%" if b.get("share") is not None else ""
                out.append(f"     {b['step']:<34} {fmt_ms(b['mean_ms']):>9} mean in {b['cases']} case(s)  {share}")
        for c in r["context"][:4]:
            if c["ratio"] and (c["ratio"] >= 1.5 or c["ratio"] <= 0.67):
                out.append(f"   {c['trait']}: median {fmt_ms(c['with'])} (n={c['n_with']}) vs {fmt_ms(c['without'])} without (n={c['n_without']})")
        out.append("")
    if batches:
        out.append(f"Batches (the Library's headless players): {len(batches)}, "
                   f"median run {fmt_ms(statistics.median([b['end'] - b['start'] for b in batches]))}")
    out.append(f"The journal's own cost: {overhead['records']} records, {overhead['record_ms']} ms making them, "
               f"{overhead['flush_ms']} ms writing them (longest write {overhead['flush_max_ms']} ms, on each plugin's own thread "
               f"or the app's journal thread, never in a frame), {overhead['bytes'] / 1024:.0f} KB on disk (player and app)")
    return "\n".join(out)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dir", default=default_dir())
    ap.add_argument("--since", default="7d", help="7d, 12h, 2026-11-19 (default 7d)")
    ap.add_argument("--until")
    ap.add_argument("--top", type=int, default=20)
    ap.add_argument("--action", help="only this action")
    ap.add_argument("--json", help="write the machine-readable report here (every action with its steps)")
    a = ap.parse_args(argv)
    now = dt.datetime.now().timestamp() * 1000
    since, until = parse_when(a.since, now), parse_when(a.until, now)
    runs = load(a.dir, since, until)
    if not runs:
        print(f"No journal in {a.dir} for that period.")
        return 1
    actions, costs, batches, by_file = build(runs)
    rows, overhead = report(actions, costs, batches, by_file, a.top, a.action)
    period = f"since {dt.datetime.fromtimestamp(since / 1000):%Y-%m-%d %H:%M}" if since else "all"
    print(text(rows, overhead, actions, by_file, batches, a.top, period))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump({"period": {"since": since, "until": until}, "instant_ms": INSTANT_MS, "ranked": rows,
                       "overhead": overhead, "batches": batches,
                       "actions": sorted(({k: (sorted(v) if isinstance(v, set) else v) for k, v in x.items() if k not in ("done",)} for x in actions.values()), key=lambda x: x["w"])},
                      f, indent=1, default=str)
        print(f"\nEvery action, with its steps: {a.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
