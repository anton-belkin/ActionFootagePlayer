"""tools/stream/fmp4time.py: segments of several renders on one timeline. Boxes built here as afp-render's HLS muxer
writes them (an init.mp4 with an empty edit per track for where its render starts, segments whose tfdt start at 0).
From the repository's folder: python3 tests/fmp4time_test.py"""

import json
import os
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools", "stream"))
import fmp4time as ft  # noqa: E402

VTS, ATS = 12800, 48000  # the video's and the audio's timescales, as afp-render's (1 s segments)


def box(typ, body):
    return struct.pack(">I4s", 8 + len(body), typ) + body


def full(typ, version, body):
    return box(typ, bytes([version, 0, 0, 0]) + body)


def init(video_ms, audio_ms):
    """An init.mp4 whose render starts video_ms / audio_ms into the clip (the movie's timescale: ms)"""
    def trak(tid, ts, empty):
        tkhd = full(b"tkhd", 0, struct.pack(">III", 0, 0, tid) + b"\0" * 68)
        elst = full(b"elst", 0, struct.pack(">I", 2) + struct.pack(">Iihh", empty, -1, 1, 0) + struct.pack(">Iihh", 0, 0, 1, 0))
        mdhd = full(b"mdhd", 0, struct.pack(">IIII", 0, 0, ts, 0) + b"\0" * 4)
        return box(b"trak", tkhd + box(b"edts", elst) + box(b"mdia", mdhd))
    mvhd = full(b"mvhd", 0, struct.pack(">IIII", 0, 0, 1000, 0) + b"\0" * 80)
    return box(b"ftyp", b"iso6\0\0\0\0") + box(b"moov", mvhd + trak(1, VTS, video_ms) + trak(2, ATS, audio_ms))


def segment(video_tfdt, audio_tfdt, version=1):
    def traf(tid, t):
        tfhd = full(b"tfhd", 0, struct.pack(">I", tid))
        tfdt = full(b"tfdt", version, struct.pack(">Q" if version else ">I", t))
        return box(b"traf", tfhd + tfdt)
    return box(b"styp", b"msdh") + box(b"moof", full(b"mfhd", 0, struct.pack(">I", 1)) + traf(1, video_tfdt) + traf(2, audio_tfdt)) + box(b"mdat", b"x" * 16)


def times(seg):
    return {tid: t for tid, t, _, _ in ft.tfdts(seg)}


class Boxes(unittest.TestCase):
    def test_init_starts(self):
        info = ft.init_info(init(28000, 27989))
        self.assertEqual(info["movie"], 1000)
        self.assertEqual(ft.starts(info), {1: 28 * VTS, 2: round(27.989 * ATS)})

    def test_without_empty_edits(self):
        b = ft.without_empty_edits(init(28000, 27989))
        self.assertNotIn(b"edts", b)
        self.assertEqual(ft.starts(ft.init_info(b)), {1: 0, 2: 0})
        # Sizes still add up: every box walks to its end
        self.assertEqual(sum(e - h for _, h, _, e in ft.boxes(b, 0, len(b))), len(b))

    def test_shifted_32_and_64_bits(self):
        for v in (0, 1):
            s = ft.shifted(segment(VTS, 48128, v), {1: 2 * VTS, 2: 96000})
            self.assertEqual(times(s), {1: 3 * VTS, 2: 48128 + 96000})


class Timeline(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.t = ft.Timeline(self.dir, 1.0)

    def write_init(self, video_ms, audio_ms):
        with open(os.path.join(self.dir, "init.mp4"), "wb") as f:
            f.write(init(video_ms, audio_ms))

    def test_render_from_zero_left_as_is(self):
        self.write_init(0, 0)
        s = segment(5 * VTS, 5 * ATS - 11)
        self.assertEqual(self.t.segment(5, s), s)

    def test_render_from_later_exact_with_its_init(self):
        # A render from segment 28: its init.mp4 is the one there; segment 29 is its second
        self.write_init(28000, 27989)
        out = self.t.segment(29, segment(VTS, 48128))
        self.assertEqual(times(out), {1: 29 * VTS, 2: 48128 + round(27.989 * ATS)})
        with open(os.path.join(self.dir, "edits.json")) as f:
            self.assertIn("28", json.load(f))

    def test_two_renders_one_timeline(self):
        # Render A from 0 (segments 0, 1), then render B from 2 writes init.mp4 again: both on one timeline
        self.write_init(0, 0)
        a1 = self.t.segment(1, segment(VTS, 48128))
        self.write_init(2000, 1989)
        b2 = self.t.segment(2, segment(0, 0))
        b3 = self.t.segment(3, segment(VTS, 48128))
        self.assertEqual([times(x)[1] for x in (a1, b2, b3)], [VTS, 2 * VTS, 3 * VTS])
        self.assertEqual(times(b2)[2], round(1.989 * ATS))

    def test_render_whose_init_is_gone(self):
        # Segment 13 of a render from 12, init.mp4 since written by a render from 40: the video's start for all
        self.write_init(40000, 39990)
        out = self.t.segment(13, segment(VTS, 48128))
        self.assertEqual(times(out), {1: 13 * VTS, 2: 48128 + 12 * ATS})

    def test_known_render_kept_after_init_changed(self):
        self.write_init(12000, 11990)
        self.t.segment(12, segment(0, 0))  # noted while init.mp4 was its own
        self.write_init(40000, 39990)
        out = self.t.segment(13, segment(VTS, 48128))
        self.assertEqual(times(out)[2], 48128 + round(11.99 * ATS))


if __name__ == "__main__":
    unittest.main()
