"""tools/journal_report.py on a journal written here: the times per action (median, p90, worst, the waiting), the
result taken from the right show, a Library double-click joined to the player's first frame, the app's own steps
given to the page's action they fall in, what the slow ones have in common, and the JSON. From the repository's
folder: python3 tests/journal_report_test.py"""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
import journal_report as jr  # noqa: E402

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


if __name__ == "__main__":
    unittest.main()
