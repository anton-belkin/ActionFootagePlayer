"""tools/stream/afp_stream.py (#335): a clip with the underwater colour (the dlogm plugin's curve uw) streams with it, as
gyroflow-export renders it: the player's shader (shaders/underwater_hlg.glsl, baked where the dlogm plugin has it by
make_looks.py) made HLG like a log curve's look, its numbers per frame from afp-uw (vs-gyroflow's uwcolor.rs: measured
and eased as an export from the render's start) piped into afp-render -uw, with the clip's strength and highlights as
the player has them (render_args' dlogm.uw_strength, uw_highlights: gyroflow/settings.lua uw_of). Renders made before
of other clips keep their fingerprint. Nothing runs on a GPU: afp-render and afp-uw are stood in for.
From the repository's folder: python3 tests/stream_uw_test.py"""

import json
import os
import re
import sys
import tempfile
import unittest

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "stream"))
import afp_stream as st  # noqa: E402
import make_looks  # noqa: E402


def args_of(curve="uw", strength=0.6, highlights=True, offset=None, length=None):
    """render_args' answer as the gyroflow plugin writes it (main.lua's dump), for a plain clip with the colour on"""
    dl = {"curve": curve, "look": "hdr", "log": False}
    if curve == "uw" and strength is not None:
        dl.update(uw_strength=strength, uw_highlights=highlights)
    a = {"clip": "/library/p/Dive/GX010042.MP4", "render": {"stabilize": True, "kind": "file", "user_data": {"lens": "stabilized"},
         "settings": {"denoise": False}}, "dlogm": dl}
    if offset is not None:
        a.update(offset=offset, length=length)
    return a


class Pipe:
    closed = False

    def close(self):
        self.closed = True


class Fake:
    """subprocess.Popen standing in for afp-uw and afp-render"""
    calls = []

    def __init__(self, args, **kw):
        self.args, self.kw, self.stdout = args, kw, Pipe()
        Fake.calls.append(self)

    def poll(self):
        return 0

    def wait(self):
        return 0


class Stream(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.player = os.path.join(cls.tmp.name, "player")
        make_looks.make(os.path.join(cls.player, "dlogm"))

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def setUp(self):
        self.saved = {k: getattr(st, k) for k in ("PLAYER", "DENOISE", "info", "clip_dir", "shader_for", "trim_cache", "log", "BIN", "GENERATED")}
        self.saved_popen = st.subprocess.Popen
        st.PLAYER = self.player
        st.BIN = os.path.join(ROOT, "tools", "stream")
        st.GENERATED = os.path.join(self.tmp.name, "generated")
        st.DENOISE = "off"
        st.subprocess.Popen = Fake
        st.trim_cache = lambda: None
        self.logged = []
        st.log = lambda *a: self.logged.append(" ".join(map(str, a)))
        self.out = tempfile.mkdtemp(dir=self.tmp.name)
        st.clip_dir = lambda clip: self.out
        st.shader_for = lambda clip, out, i: {"shader": os.path.join(out, "warp.glsl"), "output": [3840, 2160], "mode": "grid"}
        Fake.calls = []

    def tearDown(self):
        for k, v in self.saved.items():
            setattr(st, k, v)
        st.subprocess.Popen = self.saved_popen
        st.jobs.clear()

    def render(self, a, n=0, upto=None, look=None, duration=20.0):
        offset = float(a.get("offset") or 0)
        st.info = lambda clip: {"args": a, "source": a["clip"], "offset": offset, "stream": {"width": 3840, "height": 2160},
                                "duration": duration, "fps": 59.94, "seg": 1.001, "count": 20}
        clip = {"id": "abc", "name": "Dive/GX010042.MP4", "path": a["clip"]}
        if look:
            clip = st.with_look(clip, look)
        Fake.calls = []
        st.start_render(clip, n, upto)
        return Fake.calls

    def test_uw_streams_with_its_shader_and_numbers(self):
        calls = self.render(args_of())
        self.assertEqual(len(calls), 2, "afp-uw, then afp-render")
        uw, render = calls
        cmd = render.args
        shader = st.curve_shader("uw")
        self.assertTrue(shader and os.path.exists(shader))
        self.assertEqual(os.path.basename(shader), "underwater_hlg.glsl")
        with open(shader) as f, open(os.path.join(ROOT, "shaders", "underwater_hlg.glsl")) as g:
            self.assertEqual(f.read(), g.read(), "the player's shader, as it is")
        self.assertIn(shader, cmd)
        self.assertIn("--hlg", cmd)
        self.assertEqual(cmd[cmd.index("-uw") + 1], "-")
        # afp-uw's lines are afp-render's stdin; the stream keeps no end of the pipe
        self.assertIs(render.kw.get("stdin"), uw.stdout)
        self.assertTrue(uw.stdout.closed)
        self.assertFalse(any("no look for the curve" in x for x in self.logged), self.logged)

    def test_afp_uw_gets_the_render_span_and_the_clips_settings(self):
        uw = self.render(args_of(strength=0.6, highlights=True))[0].args
        self.assertEqual(os.path.basename(uw[0]), "afp-uw")
        self.assertEqual(uw[1], "/library/p/Dive/GX010042.MP4")
        self.assertEqual((float(uw[2]), float(uw[3])), (0.0, 20.0))
        self.assertEqual(uw[uw.index("--strength") + 1], "0.6")
        self.assertEqual(uw[uw.index("--highlights") + 1], "yes")
        # From segment 3 to 5 of a moment 100 s into its clip: the file's times, as afp-render's -ss and -t
        calls = self.render(args_of(strength=1, highlights=False, offset=100.0, length=12.0), n=3, upto=5, duration=12.0)
        uw, cmd = calls[0].args, calls[1].args
        start = float(cmd[cmd.index("-ss") + 1])
        self.assertAlmostEqual(float(uw[2]), start)
        self.assertAlmostEqual(float(uw[3]), 100.0 + 5 * 1.001)
        self.assertAlmostEqual(start + float(cmd[cmd.index("-t") + 1]), float(uw[3]) - 0.5 / 59.94, places=5)
        self.assertEqual(uw[uw.index("--strength") + 1], "1")
        self.assertEqual(uw[uw.index("--highlights") + 1], "no")

    def test_the_tv_looks_follow_its_hlg(self):
        cmd = self.render(args_of(), look="sdr")[1].args
        self.assertIn("--sdr", cmd)
        self.assertTrue(cmd[-1].endswith("nbcu_hlg2sdr.glsl"))
        cmd = self.render(args_of(), look="pqg2.5r30")[1].args
        self.assertIn("--pq", cmd)
        self.assertIn("-every", cmd)
        self.assertEqual(os.path.basename(cmd[-1]), "hlg_gain_2.5.glsl")
        self.assertLess(cmd.index(st.curve_shader("uw")), len(cmd) - 1, "the gain after the colour's HLG")

    def test_no_afp_uw_for_other_curves(self):
        for curve in ("dlogm", "gplog2", ""):
            calls = self.render(args_of(curve))
            self.assertEqual(len(calls), 1, curve)
            self.assertNotIn("-uw", calls[0].args)

    def test_fingerprints(self):
        # A uw clip renders again (it was as recorded); the others keep theirs (the NAS's cache stays)
        a = args_of()
        st.info = lambda clip: {"args": a}
        self.assertIn("|look:uw", st.fingerprint({"id": "abc"}))
        b = args_of(strength=0.3)
        st.info = lambda clip: {"args": b}
        other = st.fingerprint({"id": "abc"})
        st.info = lambda clip: {"args": a}
        self.assertNotEqual(other, st.fingerprint({"id": "abc"}), "a change of strength renders again")
        st.info = lambda clip: {"args": args_of("dlogm")}
        self.assertNotRegex(st.fingerprint({"id": "abc"}), r"\|(look|dn|uw)")

    def test_answers_without_the_strength_are_asked_again(self):
        saved = st.ask_player, st.ARGS
        asked = []
        st.ask_player = lambda clip: asked.append(clip["id"]) or {"clip": "x"}
        st.ARGS = tempfile.mkdtemp(dir=self.tmp.name)
        try:
            for cid, a in (("old", args_of(strength=None)), ("new", args_of()), ("log", args_of("dlogm"))):
                a["deps"] = {}
                with open(os.path.join(st.ARGS, cid + ".json"), "w") as f:
                    json.dump(a, f)
                st._args.pop(cid, None)
                st.player_args({"id": cid})
            self.assertEqual(asked, ["old"])
        finally:
            st.ask_player, st.ARGS = saved
            st._args.clear()

    def test_catalog_says_hdr(self):
        a = args_of()
        i = {"args": a, "source": a["clip"], "offset": 0.0, "duration": 10.0, "stream": {}}
        c = {"id": "x", "kind": "moment", "project": "/library/p", "folder": "", "path": "/library/p/_Moments/m.afplink.mp4"}
        self.assertEqual(st.catalog_entry(c, i, "http://nas")["look"], "hdr")

    def test_afp_render_takes_the_options(self):
        with open(os.path.join(ROOT, "tools", "stream", "afp-render.c")) as f:
            src = f.read()
        takes = set(re.findall(r'!strcmp\(argv\[i\], "([^"]+)"\)', src))
        for look in (None, "pqx1000g2r30", "sdr"):
            for x in self.render(args_of(), look=look)[1].args:
                if x.startswith("-") and len(x) > 1 and not re.match(r"^-?\d", x):
                    self.assertIn(x, takes, f"afp-render doesn't take {x}")
        # afp-render sets the parameters the shader has
        with open(os.path.join(ROOT, "shaders", "underwater_hlg.glsl")) as f:
            params = re.findall(r"^//!PARAM (\w+)", f.read(), re.M)
        m = re.search(r"UW_PARAMS\[6\] = \{([^}]*)\}", src)
        self.assertEqual(re.findall(r'"(\w+)"', m.group(1)), params)

    def test_afp_uw_takes_the_options(self):
        with open(os.path.join(ROOT, "vs-gyroflow", "src", "bin", "afp-uw.rs")) as f:
            src = f.read()
        uw = self.render(args_of())[0].args
        for x in uw:
            if x.startswith("--"):
                self.assertIn(f'"{x}" =>', src, f"afp-uw doesn't take {x}")

    def test_the_image_has_afp_uw(self):
        with open(os.path.join(ROOT, "tools", "nas-helper", "Dockerfile")) as f:
            d = f.read()
        self.assertRegex(d, r"for f in [^;]*\bafp-uw\b[^;]*; do")
        self.assertIn("/opt/afp/bin/afp-uw", d)


if __name__ == "__main__":
    unittest.main()
