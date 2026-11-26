#!/usr/bin/env python3
"""What happened, as a timeline: reads the usage trace (docs/design/usage-trace.md) — the responsiveness journal's records
with what they were about — over a stretch of time and prints, in order, who acted (anton, claude, ipc), in which app,
what they did (the key, the button's words, the file and the position), what the apps showed (OSD messages, cards,
dialogs, toasts, the answers given), the settings changed (old → new), the warnings and errors, and the markers
("something's wrong here") with their notes and pictures. For a person reading what happened, and for the file-bug
skill (.claude/skills/file-bug), which turns a stretch of it into an issue.

    tools/trace_report.py                          # the last 30 minutes
    tools/trace_report.py --marks                  # the markers, newest first (1 = the last one)
    tools/trace_report.py --mark last              # around the last marker: 5 min before it, 1 min after
    tools/trace_report.py --mark 2 --before 10m --after 2m
    tools/trace_report.py --since 2026-11-26T14:00 --until 2026-11-26T14:20 --actor anton
    tools/trace_report.py --last 2h --app library --json stretch.json

Plain Python 3, no packages. Reads only; the journal never leaves this computer.
"""

import argparse
import datetime as dt
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import journal_report as jr  # noqa: E402  (the journal's folder, the times read)

# What the timeline shows: the person's actions and what they were about; the timings stay journal_report.py's
STEPS = {"start-file", "first-frame", "handoff", "page-start"}


def ms_of(span):
    """'30m', '2h', '90s', '1d' -> ms"""
    span = span.strip()
    unit = {"s": 1, "m": 60, "h": 3600, "d": 86400}.get(span[-1:])
    if unit is None:
        return float(span) * 60000
    return float(span[:-1]) * unit * 1000


def app_of(name):
    return "library" if name.startswith("library-") else ("batch" if name.startswith("batch-") else "player")


def read(folder, since, until):
    """The records of each run in the period, and each run's header (whenever it was written)"""
    runs = {}
    names = sorted(os.listdir(folder)) if os.path.isdir(folder) else []
    for name in names:
        if not name.endswith(".jsonl"):
            continue
        path = os.path.join(folder, name)
        if since and os.path.getmtime(path) * 1000 < since:
            continue
        run, recs = {}, []
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
                if r.get("k") == "run" and not run:
                    run = r
                if since and r["w"] < since or until and r["w"] > until:
                    continue
                r["file_"] = name
                recs.append(r)
        if recs or run:
            runs[name] = {"app": app_of(name), "run": run, "recs": recs}
    return runs


def markers(folder):
    """Every marker in the folder, newest first, with its note and its picture's state"""
    out, notes = [], {}
    for name in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        if not name.endswith(".jsonl"):
            continue
        with open(os.path.join(folder, name), encoding="utf-8", errors="replace") as f:
            for line in f:
                if '"k":"mark' not in line and '"k": "mark' not in line:
                    continue
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if r.get("k") == "mark":
                    r["file_"] = name
                    out.append(r)
                elif r.get("k") == "mark-note":
                    notes.setdefault(r.get("id"), {}).update({k: v for k, v in r.items() if k not in ("k", "w", "s", "id")})
    for m in out:
        m.update({"note_" + k: v for k, v in notes.get(m.get("id"), {}).items()})
        shot = m.get("shot")
        m["shot_exists"] = bool(shot) and os.path.exists(shot)
    out.sort(key=lambda m: -m["w"])
    for i, m in enumerate(out, 1):
        m["n"] = i
    return out


def pick_mark(marks, which):
    if which in ("last", "latest"):
        return marks[0] if marks else None
    if which.isdigit():
        n = int(which)
        return marks[n - 1] if 0 < n <= len(marks) else None
    return next((m for m in marks if m.get("id") == which), None)


def clock(w):
    return dt.datetime.fromtimestamp(w / 1000).strftime("%H:%M:%S.") + f"{int(w % 1000):03d}"


def mmss(pos):
    if not isinstance(pos, (int, float)):
        return None
    m, s = divmod(pos, 60)
    return f"{int(m)}:{s:04.1f}" if m < 60 else f"{int(m // 60)}:{int(m % 60):02d}:{s:04.1f}"


def short(v, n=120):
    if isinstance(v, (dict, list)):
        v = json.dumps(v, ensure_ascii=False)
    v = "" if v is None else str(v)
    v = " ".join(v.split())
    return v if len(v) <= n else v[: n - 1] + "…"


def timeline(runs, actor=None, app=None):
    """The records to show, in time order, each with app, actor and its line; and what's around them"""
    acts = {}
    for name, x in runs.items():
        for r in x["recs"]:
            if r.get("k") == "act" and r.get("a"):
                acts[r["a"]] = r
    rows = []
    files, problems, marks, actors = [], [], [], set()
    for name, x in runs.items():
        run_actor = x["run"].get("actor") or "?"
        for r in x["recs"]:
            k = r.get("k")
            act = acts.get(r.get("a")) if r.get("a") else None
            who = r.get("actor") or (act.get("actor") if act and act.get("actor") else None) or run_actor
            r["actor_"], r["app_"] = who, x["app"]
            if k == "step" and r.get("n") not in STEPS and not str(r.get("n", "")).startswith("invoke-failed"):
                continue
            if k in ("show", "done", "io", "cost", "load", "capped"):
                continue
            if actor and who != actor or app and x["app"] != app:
                continue
            actors.add(who)
            rows.append(r)
            for f in (r.get("file"), r.get("path"), r.get("clip")):
                if f and f not in files:
                    files.append(f)
            if k == "log":
                problems.append(r)
            if k == "mark":
                marks.append(r)
    rows.sort(key=lambda r: r["w"])
    problems.sort(key=lambda r: r["w"])
    marks.sort(key=lambda r: r["w"])
    return rows, files, problems, marks, actors


def line_of(r, runs, notes):
    k, n = r.get("k"), r.get("n")
    if k == "run":
        return (f"── {r.get('app', r['app_'])} started (pid {r.get('pid')}, version {r.get('version') or '?'}, "
                f"actor {r.get('actor') or '?'}{', trace on' if r.get('trace') else ''})")
    if k == "act":
        bits = [f"▶ {n}"]
        if r.get("label"):
            bits.append(f"“{short(r['label'], 60)}”")
        if r.get("key") and r.get("via") == "key":
            bits.append(f"[{r['key']}]")
        elif r.get("via") and r.get("via") not in ("click", "key"):
            bits.append(f"({r['via']})")
        if "value" in r:
            bits.append(f"= {short(r['value'], 60)}")
        if r.get("file"):
            where = os.path.basename(r["file"]) or r["file"]
            at = mmss(r.get("pos"))
            bits.append(f"on {where}" + (f" at {at}" if at else "") + (" (paused)" if r.get("paused") else ""))
        return " ".join(bits)
    if k == "text":
        extra = []
        if r.get("kind"):
            extra.append(r["kind"])
        if r.get("quiet"):
            extra.append("quiet")
        label = n + (":" + "/".join(extra) if extra else "")
        out = f"   says ({label}): “{short(r.get('text'), 200)}”"
        for f in ("detail", "subtitle", "body"):
            if r.get(f):
                out += f" — {short(r[f], 160)}"
        if r.get("value"):
            out += f" [typed: {short(r['value'], 60)}]"
        return out
    if k == "answer":
        if r.get("cancelled"):
            return f"   answered “{short(n, 60)}”: cancelled"
        return f"   answered “{short(n, 60)}”: “{short(r.get('text'), 120)}”"
    if k == "set":
        old = "—" if "old" not in r or r.get("old") is None else short(r["old"], 60)
        clip = f" ({os.path.basename(r['clip'])})" if r.get("clip") else ""
        return f"   set {n}{clip}: {old} → {short(r.get('new'), 60)}"
    if k == "tab":
        return f"   panel: {n}"
    if k == "log":
        sign = "✖" if r.get("level") in ("error", "fatal") else "⚠"
        return f"   {sign} {r.get('level')} [{r.get('prefix')}] {short(r.get('text'), 220)}"
    if k == "mark":
        note = notes.get(r.get("id"), {})
        out = f"⚑ MARKER {r.get('id')}"
        if note.get("note"):
            out += f": “{short(note['note'], 200)}”"
        elif note.get("cancelled"):
            out += " (no note)"
        if r.get("file"):
            out += f" on {os.path.basename(r['file'])}" + (f" at {mmss(r.get('pos'))}" if r.get("pos") is not None else "")
        if r.get("shot"):
            out += f"\n                                  picture: {r['shot']}" + ("" if os.path.exists(r["shot"]) else " (missing)")
        elif r.get("no_shot"):
            out += "\n                                  no picture: " + short(r["no_shot"], 120)
        for f in ("dialogs", "toast", "focus"):
            if r.get(f):
                out += f"\n                                  {f}: {short(r[f], 160)}"
        return out
    if k == "mark-note":
        return None
    if k == "step":
        if n == "start-file":
            return f"   opens {r.get('path') or '(a file)'}" + (" (on the NAS)" if r.get("where") == "nas" else "")
        if n == "first-frame":
            return f"   first frame ({jr.fmt_ms(r.get('since_ms'))} after it was asked)"
        if n == "handoff":
            return f"   handed over from the Library ({r.get('parent') or r.get('a')})"
        if str(n).startswith("invoke-failed"):
            return f"   ✖ {n}"
        return f"   {n}"
    return f"   {k} {short(n)}"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dir", default=jr.default_dir())
    ap.add_argument("--since", help="2026-11-26T14:00, 2026-11-26, or 2h (ago)")
    ap.add_argument("--until")
    ap.add_argument("--last", help="the last 30m, 2h, 1d (default 30m, without --since or --mark)")
    ap.add_argument("--marks", action="store_true", help="list the markers, newest first, and stop")
    ap.add_argument("--mark", help="around a marker: last, its number in --marks (1 = the newest), or its id")
    ap.add_argument("--before", default="5m", help="with --mark: this long before it (default 5m)")
    ap.add_argument("--after", default="1m", help="with --mark: this long after it (default 1m)")
    ap.add_argument("--actor", help="only this actor's (anton, claude, ipc, user)")
    ap.add_argument("--app", choices=["player", "library", "batch"], help="only this app's")
    ap.add_argument("--json", help="write the stretch's records here, each with its actor and app")
    a = ap.parse_args(argv)
    now = dt.datetime.now().timestamp() * 1000

    if a.marks:
        marks = markers(a.dir)
        if not marks:
            print(f"No markers in {a.dir}.")
            return 1
        for m in marks:
            when = dt.datetime.fromtimestamp(m["w"] / 1000).strftime("%Y-%m-%d %H:%M:%S")
            note = m.get("note_note") or ("(no note)" if m.get("note_cancelled") else "")
            where = f" {os.path.basename(m['file'])} at {mmss(m.get('pos'))}" if m.get("file") else ""
            shot = m.get("shot")
            pic = f"  picture: {shot}" + ("" if m["shot_exists"] else " (missing)") if shot else "  no picture"
            print(f"{m['n']:>3}  {when}  {app_of(m['file_'])}  {m.get('actor') or '?'}  {m.get('id')}{where}  “{short(note, 100)}”\n     {pic}")
        return 0

    mark = None
    if a.mark:
        marks = markers(a.dir)
        mark = pick_mark(marks, a.mark)
        if not mark:
            print(f"No marker {a.mark} in {a.dir} (tools/trace_report.py --marks lists them).")
            return 1
        since, until = mark["w"] - ms_of(a.before), mark["w"] + ms_of(a.after)
    elif a.since:
        since, until = jr.parse_when(a.since, now), jr.parse_when(a.until, now)
    else:
        since, until = now - ms_of(a.last or "30m"), jr.parse_when(a.until, now)
    runs = read(a.dir, since, until)
    rows, files, problems, marks_in, actors = timeline(runs, a.actor, a.app)
    if not rows:
        print(f"Nothing in the trace in {a.dir} for that stretch"
              + (" (is it on? journal.conf: trace=yes, docs/design/usage-trace.md)" if not any(x['run'].get('trace') for x in runs.values()) else "") + ".")
        return 1
    notes = {}
    for x in runs.values():
        for r in x["recs"]:
            if r.get("k") == "mark-note":
                notes.setdefault(r.get("id"), {}).update(r)
    # The runs in the stretch: their headers (app, version, actor), even when written before it
    print(f"Usage trace, {dt.datetime.fromtimestamp(since / 1000):%Y-%m-%d %H:%M:%S} → "
          f"{dt.datetime.fromtimestamp((until or now) / 1000):%H:%M:%S}"
          + (f", around marker {mark['id']}" if mark else "") + (f", actor {a.actor}" if a.actor else "")
          + (f", the {a.app}" if a.app else ""))
    for name, x in sorted(runs.items()):
        if not x["recs"] or a.app and x["app"] != a.app:
            continue
        h = x["run"]
        print(f"  {x['app']:<8} {name}  version {h.get('version') or '?'}  actor {h.get('actor') or '?'}"
              f"{'  trace on' if h.get('trace') else '  (trace off: timings only)'}"
              + (f"  mpv {short(h.get('mpv'), 40)}" if h.get("mpv") else ""))
    print()
    for r in rows:
        text = line_of(r, runs, notes)
        if text is None:
            continue
        print(f"{clock(r['w'])}  {short(r['actor_'], 8):<8} {r['app_']:<8} {text}")
    if files:
        print("\nFiles:")
        for f in files:
            print(f"  {f}")
    if problems:
        print(f"\nWarnings and errors: {len(problems)}")
        for r in problems[:40]:
            print(f"  {clock(r['w'])}  {r.get('level'):<5} [{r.get('prefix')}] {short(r.get('text'), 200)}")
    if marks_in:
        print("\nMarkers:")
        for r in marks_in:
            note = notes.get(r.get("id"), {})
            print(f"  {clock(r['w'])}  {r.get('id')}  “{short(note.get('note') or '', 120)}”  "
                  + (f"picture: {r['shot']}" + ("" if os.path.exists(r["shot"]) else " (missing)") if r.get("shot") else "no picture"))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump({"since": since, "until": until, "mark": mark, "actors": sorted(actors), "files": files,
                       "runs": {n: x["run"] for n, x in runs.items()}, "records": rows}, f, indent=1, default=str)
        print(f"\nThe stretch's records: {a.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
