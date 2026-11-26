"""tools/stream/afp_stream.py: an answer the player gave before the NAS Helper was updated is asked again once, when the
clip is about to play (a GP-Log clip streamed flat from an answer of an older player, though the new one sees its
curve: #339's live check). The warm-up and the catalog keep the answers they have. Nothing runs: ask_player is stood in.
From the repository's folder: python3 tests/stream_player_refresh_test.py"""

import os
import sys
import unittest

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "stream"))
import afp_stream as st  # noqa: E402


class Refresh(unittest.TestCase):
    def setUp(self):
        self.asked = []
        self.real_ask = st.ask_player

        def ask(clip):
            self.asked.append(clip["id"])
            a = {"clip": "/shares/photo/GX010242.MP4", "deps": {}, "player": st.PLAYER_STAMP, "dlogm": {"curve": "gplog"}}
            st._args[clip["id"]] = a
            return a
        st.ask_player = ask
        st._args.clear()
        st._player_checked.clear()

    def tearDown(self):
        st.ask_player = self.real_ask
        st._args.clear()
        st._player_checked.clear()

    def clip(self, cid="c1"):
        return {"id": cid, "name": "GX010242.afplink.mp4", "path": "/library/p/GX010242.afplink.mp4", "kind": "clip preview"}

    def test_an_older_players_answer_is_asked_again_once(self):
        st._args["c1"] = {"clip": "/shares/photo/GX010242.MP4", "deps": {}, "dlogm": {"curve": ""}}
        st.args_for_this_player(self.clip())
        self.assertEqual(self.asked, ["c1"])
        self.assertEqual(st.player_args(self.clip())["dlogm"]["curve"], "gplog")
        st.args_for_this_player(self.clip())
        self.assertEqual(self.asked, ["c1"], "asked once per clip, not on every segment")

    def test_this_players_answer_is_kept(self):
        st._args["c1"] = {"clip": "/shares/photo/GX010242.MP4", "deps": {}, "player": st.PLAYER_STAMP, "dlogm": {"curve": "dlogm"}}
        st.args_for_this_player(self.clip())
        self.assertEqual(self.asked, [])

    def test_a_look_variant_asks_for_its_file(self):
        st._args["c1"] = {"clip": "/shares/photo/GX010242.MP4", "deps": {}, "dlogm": {"curve": ""}}
        variant = dict(self.clip("c1~pqg2.5r30"), base=self.clip())
        st.args_for_this_player(variant)
        self.assertEqual(self.asked, ["c1"])

    def test_warm_up_keeps_older_answers(self):
        st._args["c1"] = {"clip": "/shares/photo/GX010242.MP4", "deps": {}, "dlogm": {"curve": ""}}
        self.assertEqual(st.player_args(self.clip())["dlogm"]["curve"], "")
        self.assertEqual(self.asked, [])

    def test_the_stamp_is_the_plugins_files(self):
        self.assertRegex(st.PLAYER_STAMP, r"^[0-9a-f]{16}$")
        self.assertEqual(st._player_stamp(), st.PLAYER_STAMP)


if __name__ == "__main__":
    unittest.main()
