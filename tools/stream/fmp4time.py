# SPDX-License-Identifier: GPL-3.0-or-later
"""One timeline for a clip's fMP4 segments, whichever render made them (afp_stream.py).

afp-render writes each render (from segment f, to the end or to a limit) with ffmpeg's HLS muxer, which starts every
track's time at 0 and keeps where the render really starts only in init.mp4: an empty edit per track (f s in, as the
movie's milliseconds). The next render, from elsewhere (a seek, a cached stretch ending), writes init.mp4 again with its
own. A player that reads init.mp4 once and then segments of two renders (Jellyfin's ffmpeg remuxing for a TV, Safari
after a seek) sees time go back to 0 at each render's first segment: ffmpeg's output never gets past its first second,
a TV waits forever.

So segments are served with their real time: each track's tfdt plus its render's start, and init.mp4 without the
empty edits. A render's starts come from its own init.mp4 (exact, the audio's too: its first packet isn't at the
video's), kept per render in the clip's folder (edits.json, by the render's first segment); a segment whose render's
init.mp4 is gone gets the video's start for every track (the audio then off by at most one audio frame)."""

import json
import os
import struct

CONTAINERS = {b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"moof", b"traf", b"mvex"}


def boxes(b, start, end):
    """(type, header start, body start, end) of each box in b[start:end]"""
    o = start
    while o + 8 <= end:
        size, typ = struct.unpack(">I4s", b[o:o + 8])
        h = 8
        if size == 1:
            size = struct.unpack(">Q", b[o + 8:o + 16])[0]
            h = 16
        elif size == 0:
            size = end - o
        if size < h or o + size > end:
            return
        yield typ, o, o + h, o + size
        o += size


def _field(b, p, v1_at, v0_at):
    """A full box's 32- or 64-bit field after its version byte: (value, offset, width)"""
    if b[p]:
        return struct.unpack(">Q", b[p + v1_at:p + v1_at + 8])[0], p + v1_at, 8
    return struct.unpack(">I", b[p + v0_at:p + v0_at + 4])[0], p + v0_at, 4


def init_info(b):
    """{"movie": timescale, "tracks": {track id: {"timescale", "empty": the empty edit's length in movie units}}}"""
    out = {"movie": 1000, "tracks": {}}
    for typ, _, p, e in boxes(b, 0, len(b)):
        if typ != b"moov":
            continue
        for t2, _, p2, e2 in boxes(b, p, e):
            if t2 == b"mvhd":
                out["movie"] = _field(b, p2, 20, 12)[0]
            if t2 != b"trak":
                continue
            tid, ts, empty = None, None, 0
            for t3, _, p3, e3 in boxes(b, p2, e2):
                if t3 == b"tkhd":
                    tid = _field(b, p3, 20, 12)[0]
                elif t3 == b"edts":
                    for t4, _, p4, e4 in boxes(b, p3, e3):
                        if t4 == b"elst":
                            v, n = b[p4], struct.unpack(">I", b[p4 + 4:p4 + 8])[0]
                            q = p4 + 8
                            for _ in range(n):
                                if v:
                                    dur, mt = struct.unpack(">Qq", b[q:q + 16])
                                    q += 20
                                else:
                                    dur, mt = struct.unpack(">Ii", b[q:q + 8])
                                    q += 12
                                if mt == -1:
                                    empty += dur
                elif t3 == b"mdia":
                    for t4, _, p4, e4 in boxes(b, p3, e3):
                        if t4 == b"mdhd":
                            ts = _field(b, p4, 20, 12)[0]
            if tid is not None and ts:
                out["tracks"][tid] = {"timescale": ts, "empty": empty}
    return out


def starts(info):
    """{track id: the render's start in the track's own units}, from init_info"""
    return {tid: round(t["empty"] * t["timescale"] / info["movie"]) for tid, t in info["tracks"].items()}


def without_empty_edits(b):
    """init.mp4 with each track's edit list gone (its empty edit says where one render starts; segments carry it now)"""
    def strip(start, end):
        out = b""
        for typ, h, p, e in boxes(b, start, end):
            if typ == b"edts":
                continue
            if typ in (b"moov", b"trak"):
                body = strip(p, e)
                out += struct.pack(">I4s", 8 + len(body), typ) + body
            else:
                out += b[h:e]
        return out
    return strip(0, len(b))


def tfdts(b):
    """[(track id, tfdt, offset, width)] of a segment's first moof"""
    out = []
    for typ, _, p, e in boxes(b, 0, len(b)):
        if typ != b"moof":
            continue
        for t2, _, p2, e2 in boxes(b, p, e):
            if t2 != b"traf":
                continue
            tid, found = None, None
            for t3, _, p3, e3 in boxes(b, p2, e2):
                if t3 == b"tfhd":
                    tid = struct.unpack(">I", b[p3 + 4:p3 + 8])[0]
                elif t3 == b"tfdt":
                    found = _field(b, p3, 4, 4)
            if tid is not None and found:
                out.append((tid, found[0], found[1], found[2]))
        break
    return out


def shifted(b, add):
    """The segment with add[track] added to each track's tfdt (in every moof)"""
    b = bytearray(b)
    for typ, _, p, e in boxes(b, 0, len(b)):
        if typ != b"moof":
            continue
        for t2, _, p2, e2 in boxes(b, p, e):
            if t2 != b"traf":
                continue
            tid = None
            for t3, _, p3, e3 in boxes(b, p2, e2):
                if t3 == b"tfhd":
                    tid = struct.unpack(">I", b[p3 + 4:p3 + 8])[0]
                elif t3 == b"tfdt" and tid in add:
                    v, at, w = _field(b, p3, 4, 4)
                    v += add[tid]
                    if w == 4 and v >= 1 << 32:
                        raise ValueError("tfdt past 32 bits")
                    b[at:at + w] = struct.pack(">Q" if w == 8 else ">I", v)
    return bytes(b)


class Timeline:
    """A clip's folder: its renders' starts (edits.json) and its segments with their real time"""

    def __init__(self, folder, seg_seconds):
        self.folder = folder
        self.seg = seg_seconds
        self.path = os.path.join(folder, "edits.json")

    def _load(self):
        try:
            with open(self.path) as f:
                return json.load(f)
        except (OSError, ValueError):
            return {}

    def note_init(self):
        """The render that wrote init.mp4 last: its starts kept, by its first segment (the video's start / seg)"""
        try:
            with open(os.path.join(self.folder, "init.mp4"), "rb") as f:
                info = init_info(f.read())
        except OSError:
            return None
        s = starts(info)
        video = next((t for tid, t in info["tracks"].items() if tid == 1), None) or next(iter(info["tracks"].values()), None)
        if not video:
            return None
        first = round(video["empty"] / info["movie"] / self.seg)
        known = self._load()
        if known.get(str(first)) != {str(k): v for k, v in s.items()}:
            known[str(first)] = {str(k): v for k, v in s.items()}
            tmp = self.path + ".tmp"
            with open(tmp, "w") as f:
                json.dump(known, f)
            os.replace(tmp, self.path)
        return first

    def segment(self, n, data):
        """Segment n with its real time: its render's starts added (once: a segment already in place is left)"""
        found = tfdts(data)
        if not found:
            return data
        video = next((x for x in found if x[0] == 1), found[0])
        ts = None
        try:
            with open(os.path.join(self.folder, "init.mp4"), "rb") as f:
                tracks = init_info(f.read())["tracks"]
            ts = {tid: t["timescale"] for tid, t in tracks.items()}
        except OSError:
            pass
        vts = (ts or {}).get(video[0]) or 90000
        per_seg = self.seg * vts
        into = round(video[1] / per_seg)  # segments into its render
        if into >= n and abs(video[1] - n * per_seg) < per_seg / 2:
            return data  # already on the clip's timeline (a render from 0, or served before)
        first = n - into
        known = self._load().get(str(first))
        if not known and self.note_init() == first:
            # init.mp4 is still this render's: its starts, exact
            known = self._load().get(str(first))
        if known:
            add = {int(k): v for k, v in known.items()}
        else:
            add = {tid: round(first * self.seg * (ts or {}).get(tid, vts)) for tid, _, _, _ in found}
        return shifted(data, add)
