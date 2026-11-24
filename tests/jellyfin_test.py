"""tools/stream/jellyfin.py: where moments and clip previews go, and what happens right after afp-stream starts (its
catalog has items pending and the clip previews aren't listed yet): a moment whose trip comes from its clip's preview
stays where it was, not written a second time under "No trip"; one that truly has no trip goes there once nothing is
pending. From the repository's folder: python3 tests/jellyfin_test.py"""

import json
import os
import sys
import tempfile
import types
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "stream"))
import jellyfin as jf  # noqa: E402

RU = jf.NAMES["ru"]


def moment(name, clip, folder="", project="P"):
    return {"kind": "moment", "id": f"m-{name}", "project": project, "clip": clip, "folder": folder, "name": name,
            "title": name, "start": "2026-10-23T14:48:00", "in": 1.0, "duration": 5.0, "stabilized": True, "look": "hdr"}


def preview(clip, folder, project="P"):
    return {"kind": "preview", "id": f"p-{clip}", "project": project, "clip": clip, "folder": folder, "name": clip,
            "start": "2026-10-23T14:40:00", "duration": 60.0, "stabilized": True, "look": "hdr"}


class Tree(unittest.TestCase):
    def test_trip_from_preview(self):
        out = jf.tree([moment("Скат", "GX010001"), preview("GX010001", "Египет 2026")], RU, "moments")
        self.assertEqual(list(out), ["Моменты/Египет 2026/Скат.strm"])

    def test_no_trip_once_nothing_pending(self):
        out = jf.tree([moment("Скат", "GX010001")], RU, "moments", pending=0)
        self.assertEqual(list(out), ["Моменты/Без поездки/Скат.strm"])

    def test_unknown_trip_left_out_while_pending(self):
        items = [moment("Скат", "GX010001"), moment("Риф", "GX010002", folder="Красное море"),
                 moment("Черепаха", "GX010003"), preview("GX010003", "Египет 2026")]
        out = jf.tree(items, RU, "moments", pending=1190)
        self.assertEqual(sorted(out), ["Моменты/Египет 2026/Черепаха.strm", "Моменты/Красное море/Риф.strm"])


class Run(unittest.TestCase):
    """run() three times on one folder: before the restart, just after it (pending), and once the previews are listed
    again (or not, for a clip whose preview really is gone)"""

    def setUp(self):
        self.out = tempfile.mkdtemp()
        self.cat = None
        self.fetch = jf.fetch
        jf.fetch = self.fake_fetch
        jf.log = lambda *a: None
        self.a = types.SimpleNamespace(names="ru", catalog="http://afp/catalog.json", stream="http://nas:8099",
                                       stream_local="http://afp", out=self.out, scope="moments", jellyfin=None,
                                       key_file=None, jellyfin_path=None)

    def tearDown(self):
        jf.fetch = self.fetch

    def fake_fetch(self, url, key=None, method="GET"):
        if url.endswith("/catalog.json"):
            return json.dumps(self.cat).encode()
        if "/poster/" in url:
            self.posters = getattr(self, "posters", 0) + 1
            return b"\xff\xd8jpeg " + url.encode()
        raise AssertionError(url)

    def strm(self):
        found = []
        for d, _, files in os.walk(self.out):
            found += [os.path.relpath(os.path.join(d, f), self.out) for f in files if f.endswith(".strm")]
        return sorted(found)

    def round(self, items, pending):
        self.cat = {"items": items, "pending": pending}
        jf.run(self.a)
        return self.strm()

    def test_restart_writes_no_duplicates(self):
        full = [moment("Скат", "GX010001"), preview("GX010001", "Египет 2026"), moment("Риф", "GX010002", folder="Красное море")]
        self.assertEqual(self.round(full, 0), ["Моменты/Египет 2026/Скат.strm", "Моменты/Красное море/Риф.strm"])
        # Just restarted: the previews aren't listed yet; nothing new under "Без поездки", the old files stay
        early = [moment("Скат", "GX010001"), moment("Риф", "GX010002", folder="Красное море")]
        self.assertEqual(self.round(early, 1190), ["Моменты/Египет 2026/Скат.strm", "Моменты/Красное море/Риф.strm"])
        self.assertTrue(os.path.exists(os.path.join(self.out, "Моменты/Египет 2026/Скат.nfo")))
        # Everything listed again: the same as before the restart
        self.assertEqual(self.round(full, 0), ["Моменты/Египет 2026/Скат.strm", "Моменты/Красное море/Риф.strm"])
        # The preview really gone (deleted as not interesting): its moment moves to "Без поездки"
        self.assertEqual(self.round(early, 0), ["Моменты/Без поездки/Скат.strm", "Моменты/Красное море/Риф.strm"])
        self.assertFalse(os.path.exists(os.path.join(self.out, "Моменты/Египет 2026")))

    def test_new_moment_without_trip_waits_for_pending(self):
        self.assertEqual(self.round([moment("Скат", "GX010001")], 5), [])
        self.assertEqual(self.round([moment("Скат", "GX010001")], 0), ["Моменты/Без поездки/Скат.strm"])

    def test_cover_asked_again_only_when_its_file_changed(self):
        full = [dict(moment("Скат", "GX010001"), poster_stamp="100:1"), preview("GX010001", "Египет 2026"),
                dict(moment("Риф", "GX010002", folder="Красное море"), poster_stamp="200:1")]
        self.round(full, 0)
        self.assertEqual(self.posters, 2)
        self.round(full, 0)
        self.assertEqual(self.posters, 2)
        # The moment made again: its file's stamp changed, its cover is asked for again; the other one isn't
        full[0]["poster_stamp"] = "120:2"
        self.round(full, 0)
        self.assertEqual(self.posters, 3)
        # A cover gone from the folder is fetched again whatever the stamp says
        os.remove(os.path.join(self.out, "Моменты/Красное море/Риф-poster.jpg"))
        self.round(full, 0)
        self.assertEqual(self.posters, 4)
        self.assertTrue(os.path.exists(os.path.join(self.out, "Моменты/Красное море/Риф-poster.jpg")))

    def test_cover_without_stamp_asked_every_round(self):
        # An older afp-stream gives no stamp: as before, asked each time
        items = [moment("Риф", "GX010002", folder="Красное море")]
        self.round(items, 0)
        self.round(items, 0)
        self.assertEqual(self.posters, 2)


if __name__ == "__main__":
    unittest.main()
