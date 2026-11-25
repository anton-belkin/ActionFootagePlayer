"""tools/stream/afp_stream.py's clips() (#302): a project's own folders (registry.json's own_folders: Generated/ from
before 0.11, _Exports, exports, …) are never listed as original clips, at the project's top (the library's folder or
one with Project.afproject), nor gyroflow-export's stabilized/ wherever it is; _Moments and _Clips are read as moments
and clip previews; a folder of the footage that only shares a name with one (Trip/clips) still counts. Nothing on the
NAS: a tree in a temporary folder. From the repository's folder: python3 tests/stream_folders_test.py"""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "stream"))
import afp_stream as st  # noqa: E402

FILES = [
    "Project.afproject",
    "Day 1/DJI_0001.MP4",
    "Day 1/stabilized/DJI_0001_stabilized.mp4",  # gyroflow-export beside the clip
    "Generated/2026-10-01 Exports/DJI_0001.mov",  # a project's top-level Generated/ (before 0.11)
    "stabilized/DJI_0002.mov",
    "exports/old.mp4",
    "_Exports/e.mp4",
    "_Stills/s.mov",
    "#recycle/DJI_0003.MP4",
    ".afp/x.mp4",
    "_Moments/Reef/Turtle.afplink.mp4",
    "_Clips/Day 1/DJI_0001.afplink.mp4",
    "Trip/clips/GX010001.MP4",  # a folder of the footage named like one of the project's, not at its top
    "Trip/Generated/GX010002.MP4",
    # A project inside the library's folder: its own top
    "Other/Project.afproject",
    "Other/Day 2/GX010003.MP4",
    "Other/Generated/Run Exports/GX010003.mov",
    "Other/MOMENTS/m.mp4",
]


class Clips(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        for f in FILES:
            p = os.path.join(self.tmp.name, f)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            open(p, "w").close()
        st.LIBRARY = [self.tmp.name]
        st._clips.update(at=0, list=[])

    def tearDown(self):
        self.tmp.cleanup()

    def listed(self):
        return {c["name"]: c["kind"] for c in st.clips()}

    def test_own_folders_left_out(self):
        self.assertEqual(self.listed(), {
            "Day 1/DJI_0001.MP4": "clip",
            "_Moments/Reef/Turtle.afplink.mp4": "moment",
            "_Clips/Day 1/DJI_0001.afplink.mp4": "preview",
            "Trip/clips/GX010001.MP4": "clip",
            "Trip/Generated/GX010002.MP4": "clip",
            "Other/Day 2/GX010003.MP4": "clip",
        })

    def test_the_registrys_list(self):
        # The stream reads registry.json's, not a copy of its own
        self.assertIn("generated", st.OWN_FOLDERS)
        self.assertIn("stabilized", st.OWN_FOLDERS)
        self.assertIn("#recycle", st.RECYCLE_BINS)


if __name__ == "__main__":
    unittest.main()
