"""tools/stream/afp_stream.py (#315): every log curve the player plays streams with its look to HLG (registry.json's
curves, their shaders baked by tools/stream/make_looks.py as the NAS Helper's image does), and the clip's temporal
denoise comes from the settings the player builds (render_args' render.settings), rendered with AFP_DENOISE=player:
the warp asked with its denoise passes, afp-render told to show the frame `radius` behind with the player's values;
off, a clip with the denoise says so in the log. afp-render takes every option the stream passes. Renders already made
keep their fingerprint where nothing changed (no cache wiped on the NAS by this). Nothing runs on a GPU: afp-render and
gyroflow-gpu-table are stood in for. From the repository's folder: python3 tests/stream_looks_test.py"""

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

with open(os.path.join(ROOT, "mpv", "scripts", "workspace", "registry.json")) as f:
    REGISTRY = json.load(f)
LOG = [c["id"] for c in REGISTRY["curves"] if c.get("log")]


def args_of(curve="dlogm", denoise=True, amount="full", strong=False, grain=0.25, stabilize=True):
    """render_args' answer as the gyroflow plugin writes it (main.lua's dump: render.user_data, render.settings)"""
    return {"clip": "/library/p/Day 1/A.MP4", "render": {"stabilize": stabilize, "kind": "file",
            "user_data": {"gpu_denoise": denoise and amount != "off", "lens": "stabilized"},
            "settings": {"denoise": denoise, "temporal_denoise": amount, "denoise_strong": strong, "denoise_grain": grain}},
            "dlogm": {"curve": curve, "look": "", "log": bool(curve)}}


class Fake:
    """subprocess.Popen standing in for afp-render"""
    calls = []

    def __init__(self, args, **kw):
        Fake.calls.append(args)

    def poll(self):
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
        # (the looks' variants' shaders: hlg_gain.glsl and hlg_highlights.glsl as the image has them, written beside)
        st.BIN = os.path.join(ROOT, "tools", "stream")
        st.GENERATED = os.path.join(self.tmp.name, "generated")
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

    def render(self, a, look=None):
        st.info = lambda clip: {"args": a, "source": a["clip"], "offset": 0.0, "stream": {"width": 3840, "height": 2160},
                                "duration": 10.0, "fps": 59.94, "seg": 1.001, "count": 10}
        clip = {"id": "abc", "name": "Day 1/A.MP4", "path": a["clip"]}
        if look:
            clip = st.with_look(clip, look)
        st.start_render(clip, 0)
        return Fake.calls[-1]

    def test_every_log_curve_has_its_look(self):
        for curve in LOG:
            with self.subTest(curve=curve):
                shader = st.curve_shader(curve)
                self.assertTrue(shader and os.path.exists(shader), f"{curve}: no look on the stream")
                self.assertEqual(os.path.basename(shader), os.path.basename(REGISTRY["curves"][LOG.index(curve)]["hdr"]))
                cmd = self.render(args_of(curve, denoise=False))
                self.assertIn(shader, cmd)
                self.assertIn("--hlg", cmd)
        # The SDR look follows any curve's HLG
        cmd = self.render(args_of("gplog2", denoise=False), look="sdr")
        self.assertIn("--sdr", cmd)
        self.assertTrue(cmd[-1].endswith("nbcu_hlg2sdr.glsl"))

    def test_no_curve_or_not_log(self):
        cmd = self.render(args_of("", denoise=False))
        self.assertNotIn("--hlg", cmd)
        # (the underwater colour has its look since #335: tests/stream_uw_test.py)
        cmd = self.render(args_of("hlgcam", denoise=False))
        self.assertNotIn("--hlg", cmd)
        self.assertTrue(any("no look for the curve hlgcam" in x for x in self.logged))

    def test_catalog_says_hdr_for_every_log_curve(self):
        for curve in LOG:
            a = args_of(curve)
            i = {"args": a, "source": a["clip"], "offset": 0.0, "duration": 10.0, "stream": {}}
            c = {"id": "x", "kind": "moment", "project": "/library/p", "folder": "", "path": "/library/p/_Moments/m.afplink.mp4"}
            self.assertEqual(st.catalog_entry(c, i, "http://nas")["look"], "hdr", curve)

    def test_denoise_as_the_player_has_it(self):
        st.DENOISE = "player"
        cmd = self.render(args_of(amount="light", strong=True, grain=0.5))
        self.assertEqual(cmd[cmd.index("-behind") + 1], "3")
        sets = {cmd[k + 1] for k, x in enumerate(cmd) if x == "-set"}
        self.assertEqual(sets, {"gyroflow_dn_radius=3", "gyroflow_dn_after=3", "gyroflow_dn_strong=1", "gyroflow_dn_keep=0.5"})
        cmd = self.render(args_of(amount="full"))
        self.assertEqual(cmd[cmd.index("-behind") + 1], "5")
        self.assertIn("gyroflow_dn_keep=0.25", cmd)

    def test_no_denoise_where_the_player_has_none(self):
        st.DENOISE = "player"
        for a in (args_of(denoise=False), args_of(amount="off"), args_of(stabilize=False)):
            self.assertNotIn("-behind", self.render(a))
        self.assertIsNone(st.denoise_of(args_of(denoise=False)))
        # From before the settings were in the answer: the table's request says
        a = args_of()
        del a["render"]["settings"]
        self.assertEqual(st.denoise_of(a), {"radius": 5, "strong": False, "keep": 0.25})

    def test_denoise_off_on_the_stream_says_so(self):
        st.DENOISE = "off"
        cmd = self.render(args_of())
        self.assertNotIn("-behind", cmd)
        self.assertTrue(any("temporal denoise left out" in x for x in self.logged), self.logged)

    def test_the_warp_is_asked_with_its_denoise(self):
        # shader_for's request to gyroflow-gpu-table: gpu_denoise where the stream does it
        st.shader_for = self.saved["shader_for"]
        asked = []

        class Done:
            stdout, stderr = '{"shader": "w.glsl", "output": [1920, 1080], "mode": "grid"}', ""

        saved = st.subprocess.run
        st.subprocess.run = lambda args, **kw: asked.append(json.loads(args[2])) or Done
        try:
            for mode, want in (("player", True), ("off", False)):
                st.DENOISE = mode
                out = tempfile.mkdtemp(dir=self.tmp.name)
                i = {"args": args_of(), "stream": {"width": 3840, "height": 2160}}
                st.shader_for({"name": "A"}, out, i)
                self.assertIs(asked[-1]["gpu_denoise"], want, mode)
        finally:
            st.subprocess.run = saved

    def test_renders_made_before_keep_their_fingerprint(self):
        st.DENOISE = "off"
        a = args_of("dlogm")
        st.info = lambda clip: {"args": a}
        clip = {"id": "abc"}
        self.assertNotRegex(st.fingerprint(clip), r"\|(look|dn):")
        a = args_of("gplog")
        self.assertIn("|look:gplog", st.fingerprint(clip))
        st.DENOISE = "player"
        self.assertIn("|dn:5:0:0.25", st.fingerprint(clip))

    def test_denoise_frames_are_the_players(self):
        with open(os.path.join(ROOT, "mpv", "scripts", "gyroflow", "main.lua")) as f:
            lua = f.read()
        m = re.search(r"local DENOISE_FRAMES = \{([^}]*)\}", lua)
        self.assertTrue(m)
        players = {k: int(v) for k, v in re.findall(r"(\w+) = (\d+)", m.group(1))}
        self.assertEqual(st.DENOISE_FRAMES, players)

    def test_afp_render_takes_every_option_passed(self):
        st.DENOISE = "player"
        with open(os.path.join(ROOT, "tools", "stream", "afp-render.c")) as f:
            src = f.read()
        takes = set(re.findall(r'!strcmp\(argv\[i\], "([^"]+)"\)', src))
        for a in (args_of(), args_of("gplog")):
            for look in (None, "pqx1000g2r30", "sdr"):
                for x in self.render(a, look):
                    if x.startswith("-") and not re.match(r"^-?\d", x):
                        self.assertIn(x, takes, f"afp-render doesn't take {x}")

    def test_the_image_bakes_the_looks(self):
        with open(os.path.join(ROOT, "tools", "nas-helper", "Dockerfile")) as f:
            self.assertIn("tools/stream/make_looks.py /opt/afp/player/dlogm", f.read())
        with open(os.path.join(ROOT, "tools", "nas-helper", "Dockerfile.dockerignore")) as f:
            ignore = f.read()
        self.assertIn("!tools/make_hdr_lut.py", ignore)


if __name__ == "__main__":
    unittest.main()
