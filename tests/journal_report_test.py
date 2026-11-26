"""tools/journal_report.py on a journal written here: the times per action (median, p90, worst, the waiting), the
result taken from the right show, a Library double-click joined to the player's first frame, the app's own steps
given to the page's action they fall in, what the slow ones have in common, and the JSON. From the repository's
folder: python3 tests/journal_report_test.py

And tools/trace_report.py (the usage trace, docs/design/usage-trace.md): the timeline of a stretch with who acted,
the words shown, the settings changed, the warnings, the markers with their notes and pictures; by actor, by app,
around a marker."""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
import journal_report as jr  # noqa: E402
import trace_report as tr  # noqa: E402

T = 1_795_000_000_000.0


def write(folder, name, recs):
    with open(os.path.join(folder, name), "w") as f:
        for r in recs:
            f.write(json.dumps(r) + "\n")
        f.write("not json\n")


class Report(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        p = []
        p.append({"k": "run", "w": T, "app": "player", "pid": 1, "start": T - 2000, "s": "workspace"})
        p.append({"k": "act", "a": "P1.workspace.1", "n": "player/start", "w": T - 2000, "via": "core", "s": "workspace"})
        p.append({"k": "step", "a": "P1.workspace.1", "n": "start-file", "w": T + 100, "where": "nas", "nth": 1, "s": "workspace"})
        p.append({"k": "step", "a": "P1.workspace.1", "n": "proc:gyroflow-gpu-table", "w": T + 200, "d": 1500, "s": "gyroflow"})
        p.append({"k": "step", "a": "P1.workspace.1", "n": "file-loaded", "w": T + 300, "size": 2e9, "s": "workspace"})
        p.append({"k": "show", "a": "P1.workspace.1", "n": "frame", "w": T + 2000, "s": "workspace"})
        # Ctrl+p five times: 100, 200, 300, 400, 1000 ms to its picture; an osd at once
        for i, d in enumerate([100, 200, 300, 400, 1000]):
            w = T + 10000 * (i + 1)
            a = f"P1.gyroflow.{i + 1}"
            p.append({"k": "act", "a": a, "n": "gyroflow/still", "w": w, "via": "key", "d": 2, "s": "gyroflow"})
            p.append({"k": "show", "a": a, "n": "osd", "w": w + 5, "s": "gyroflow"})
            p.append({"k": "show", "a": a, "n": "picture", "w": w + d, "s": "gyroflow"})
            # Another plugin drawing late, the action only the last one asked: not its result
            p.append({"k": "show", "a": a, "n": "overlay:side_panel", "w": w + 5000, "weak": True, "s": "side_panel"})
            p.append({"k": "step", "a": a, "n": "proc:gyroflow-gpu-table", "w": w + 10, "d": d - 20, "s": "gyroflow"})
        # A file opened from the Library: its handoff names the Library's action
        p.append({"k": "act", "a": "P1.workspace.9", "n": "file/open", "w": T + 70000, "via": "core", "s": "workspace"})
        p.append({"k": "step", "a": "P1.workspace.9", "n": "handoff", "w": T + 70000, "parent": "L2.p3", "asked": T + 69000, "s": "workspace"})
        p.append({"k": "step", "a": "P1.workspace.9", "n": "start-file", "w": T + 70001, "where": "local", "s": "workspace"})
        p.append({"k": "show", "a": "P1.workspace.9", "n": "frame", "w": T + 70800, "s": "workspace"})
        p.append({"k": "cost", "w": T + 80000, "records": 40, "bytes": 6000, "rec_ms": 1.0, "flush_ms": 3.0, "flush_max_ms": 0.8, "s": "gyroflow"})
        write(self.dir, "player-20261118-000000-1.jsonl", p)
        lib = []
        lib.append({"k": "run", "w": T, "app": "library", "pid": 2, "s": "app"})
        lib.append({"k": "act", "a": "L2.p3", "n": "dblclick:pick", "w": T + 69000, "via": "dblclick", "s": "page"})
        lib.append({"k": "show", "a": "L2.p3", "n": "paint", "w": T + 69016, "s": "page"})
        lib.append({"k": "done", "a": "L2.p3", "n": "dblclick:pick", "w": T + 69000, "settled": T + 69030, "paint": T + 69016,
                    "parts": {"invoke:open_items": [1, 12.0, 12.0], "handlers": [1, 4.0, 4.0]}, "s": "page"})
        lib.append({"k": "act", "a": "L2.p4", "n": "click:view", "w": T + 90000, "via": "click", "s": "page"})
        lib.append({"k": "done", "a": "L2.p4", "n": "click:view", "w": T + 90000, "settled": T + 90400, "paint": T + 90020,
                    "parts": {"invoke:scan": [1, 350.0, 350.0]}, "s": "page"})
        # The app's own step, inside the click's time: its
        lib.append({"k": "step", "n": "scan", "w": T + 90010, "d": 330, "where": "nas", "s": "app"})
        write(self.dir, "library-20261118-000000-2.jsonl", lib)

    def run_report(self):
        runs = jr.load(self.dir, None, None)
        actions, costs, batches, by_file = jr.build(runs)
        rows, overhead = jr.report(actions, costs, batches, by_file, 20)
        return actions, {r["action"]: r for r in rows}, overhead

    def test_times(self):
        _, rows, _ = self.run_report()
        r = rows["gyroflow/still"]
        self.assertEqual(r["count"], 5)
        self.assertEqual(r["median_ms"], 300)
        self.assertEqual(r["worst_ms"], 1000)
        self.assertAlmostEqual(r["p90_ms"], 760, places=0)
        # past 100 ms each: 0 + 100 + 200 + 300 + 900
        self.assertEqual(r["waiting_ms"], 1500)
        self.assertEqual(r["response_median_ms"], 5)
        self.assertEqual(r["lost_ms"], 2000)

    def test_start_and_breakdown(self):
        _, rows, _ = self.run_report()
        r = rows["player/start"]
        self.assertEqual(r["median_ms"], 4000)
        steps = {b["step"]: b for b in r["breakdown"]}
        self.assertEqual(steps["proc:gyroflow-gpu-table"]["mean_ms"], 1500)
        self.assertEqual(steps["@file-loaded"]["mean_ms"], 2300)

    def test_library_to_player(self):
        actions, rows, _ = self.run_report()
        r = rows["dblclick:pick → player"]
        # From the click (69000) to the player's first frame (70800)
        self.assertEqual(r["median_ms"], 1800)
        x = actions["L2.p3"]
        self.assertEqual(x["player"], "P1.workspace.9")
        self.assertTrue(any(s["n"] == "player:start-file" for s in x["steps"]))
        # Not counted twice: the player's side of it isn't a row of its own
        self.assertNotIn("file/open", rows)

    def test_app_steps_and_context(self):
        actions, rows, _ = self.run_report()
        x = actions["L2.p4"]
        self.assertEqual(x["result_ms"], 400)
        self.assertTrue(any(s["n"] == "app:scan" for s in x["steps"]))
        self.assertEqual(x["ctx"]["where"], "nas")
        r = rows["gyroflow/still"]
        first = [c for c in r["context"] if c["trait"] == "first of its kind in the run"]
        self.assertEqual(first[0]["with"], 100)

    def test_json_and_text(self):
        out = os.path.join(self.dir, "r.json")
        self.assertEqual(jr.main(["--dir", self.dir, "--since", "100000d", "--json", out]), 0)
        d = json.load(open(out))
        self.assertEqual(d["ranked"][0]["action"], "player/start")
        self.assertEqual(d["overhead"]["records"], 40)
        self.assertTrue(any(a["id"] == "L2.p3" for a in d["actions"]))

    def test_parse_when(self):
        now = T
        self.assertEqual(jr.parse_when("2d", now), T - 2 * 86400 * 1000)
        self.assertIsNone(jr.parse_when(None, now))


class Trace(unittest.TestCase):
    """tools/trace_report.py: a player run by anton with the trace on, a Library run by claude, two markers"""

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.shot = os.path.join(self.dir, "mark-20261126-140000-1-1.png")
        open(self.shot, "wb").write(b"png")
        p = [
            {"k": "run", "w": T, "app": "player", "pid": 1, "version": "0.14.2", "actor": "anton", "trace": True, "s": "workspace"},
            {"k": "act", "a": "P1.gyroflow.1", "n": "gyroflow/level", "w": T + 1000, "via": "key", "key": "Ctrl+l",
             "file": "/Volumes/DJI_Inbox/DJI_0042.MP4", "pos": 83.5, "paused": False, "s": "gyroflow"},
            {"k": "text", "n": "osd", "text": "Level: strong", "a": "P1.gyroflow.1", "w": T + 1010, "s": "gyroflow"},
            {"k": "set", "n": "gyroflow.level", "old": "medium", "new": "strong", "a": "P1.gyroflow.1", "w": T + 1020, "s": "workspace"},
            {"k": "log", "level": "warn", "prefix": "gyroflow", "text": "gpu table late", "w": T + 1500, "s": "workspace"},
            {"k": "act", "a": "P1.side_panel.1", "n": "side_panel/mark", "w": T + 2000, "via": "key", "key": "Alt+b", "s": "side_panel"},
            {"k": "mark", "id": "M1.side_panel.1", "a": "P1.side_panel.1", "w": T + 2001, "shot": self.shot, "actor": "anton",
             "file": "/Volumes/DJI_Inbox/DJI_0042.MP4", "pos": 84.0, "s": "side_panel"},
            {"k": "text", "n": "dialog", "text": "Something's wrong here", "a": "P1.side_panel.1", "w": T + 2100, "s": "side_panel"},
            {"k": "mark-note", "id": "M1.side_panel.1", "note": "the horizon tilts after Ctrl+l", "w": T + 6000, "s": "side_panel"},
            {"k": "show", "n": "frame", "a": "P1.gyroflow.1", "w": T + 1100, "s": "workspace"},
            # A key while an IPC client said who it is: its action
            {"k": "act", "a": "P1.workspace.9", "n": "pause", "w": T + 7000, "via": "core", "actor": "ipc", "s": "workspace"},
        ]
        write(self.dir, "player-20261126-140000-1.jsonl", p)
        lib = [
            {"k": "run", "w": T + 500, "app": "library", "pid": 2, "version": "0.1.0", "actor": "claude", "trace": True, "s": "app"},
            {"k": "act", "a": "L2.p1", "n": "click:view", "label": "Sources", "w": T + 3000, "via": "click", "s": "page"},
            {"k": "text", "n": "toast", "text": "Taken in: 3 files", "a": "L2.p1", "w": T + 3100, "s": "page"},
            {"k": "log", "level": "error", "prefix": "invoke:recent", "text": "the share isn't there", "w": T + 3200, "s": "page"},
            {"k": "set", "n": "source.rotation", "clip": "Day 1/A.MP4", "old": None, "new": 90, "w": T + 3300, "s": "app"},
            {"k": "mark", "id": "L2.m1", "w": T + 60 * 60000, "via": "click", "actor": "claude", "shot": None, "no_shot": "no picture here", "s": "page"},
        ]
        write(self.dir, "library-20261126-140000-2.jsonl", lib)

    def run_report(self, *args):
        import contextlib
        import io
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = tr.main(["--dir", self.dir, *args])
        return code, out.getvalue()

    def test_marks_newest_first(self):
        marks = tr.markers(self.dir)
        self.assertEqual([m["id"] for m in marks], ["L2.m1", "M1.side_panel.1"])
        self.assertEqual(marks[1]["note_note"], "the horizon tilts after Ctrl+l")
        self.assertTrue(marks[1]["shot_exists"])
        code, out = self.run_report("--marks")
        self.assertEqual(code, 0)
        self.assertIn("M1.side_panel.1", out)
        self.assertIn(self.shot, out)

    def test_around_a_marker(self):
        code, out = self.run_report("--mark", "2", "--before", "5m", "--after", "1m")
        self.assertEqual(code, 0, out)
        # Who, which app, what: the key and the file and position, the words shown, the setting, the warning
        self.assertIn("anton    player   ▶ gyroflow/level [Ctrl+l] on DJI_0042.MP4 at 1:23.5", out)
        self.assertIn("says (osd): “Level: strong”", out)
        self.assertIn("set gyroflow.level: medium → strong", out)
        self.assertIn("⚠ warn [gyroflow] gpu table late", out)
        self.assertIn("⚑ MARKER M1.side_panel.1: “the horizon tilts after Ctrl+l”", out)
        self.assertIn("picture: " + self.shot, out)
        self.assertIn("claude   library  ▶ click:view “Sources”", out)
        self.assertIn("says (toast): “Taken in: 3 files”", out)
        self.assertIn("✖ error [invoke:recent] the share isn't there", out)
        self.assertIn("set source.rotation (A.MP4): — → 90", out)
        self.assertIn("ipc      player   ▶ pause (core)", out)
        self.assertIn("version 0.14.2", out)
        self.assertIn("/Volumes/DJI_Inbox/DJI_0042.MP4", out.split("Files:")[1])
        # The timings stay journal_report's: no shows
        self.assertNotIn("frame", out.split("Files:")[0].replace("first frame", ""))
        # The Library's marker an hour later is outside
        self.assertNotIn("L2.m1", out)

    def test_by_actor_and_app(self):
        code, out = self.run_report("--since", "100000d", "--actor", "anton")
        self.assertEqual(code, 0)
        self.assertIn("gyroflow/level", out)
        self.assertNotIn("click:view", out)
        self.assertNotIn("▶ pause", out)
        code, out = self.run_report("--since", "100000d", "--app", "library", "--json", os.path.join(self.dir, "t.json"))
        self.assertEqual(code, 0)
        self.assertNotIn("gyroflow/level", out)
        d = json.load(open(os.path.join(self.dir, "t.json")))
        self.assertEqual(d["actors"], ["claude"])
        self.assertTrue(all(r["app_"] == "library" for r in d["records"]))

    def test_no_marker(self):
        code, out = self.run_report("--mark", "9")
        self.assertEqual(code, 1)
        self.assertIn("No marker 9", out)


if __name__ == "__main__":
    unittest.main()
