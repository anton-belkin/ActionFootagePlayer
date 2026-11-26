#!/usr/bin/env python3
"""The sample clip that ships with the player: a part of a camera's clip, its video made smaller, the camera's own
tracks kept.

    make_sample_clip.py <clip.MP4> --from S --to S --out <sample.mp4> [--height 1080] [--crf 24] [--preset slow]
    make_sample_clip.py --swap <cut.mp4> <encoded.mp4> --out <sample.mp4>     only the last step, for trying encodes
    make_sample_clip.py --check <cut.mp4> <sample.mp4>                        only the checks

1. afp-cut keeps the part without re-encoding (vs-gyroflow/src/cut.rs): every sample of every track byte for byte, the
   DJI clip header (product, lens, readout time, colour mode: only in the first djmd sample) carried to the new start.
2. ffmpeg encodes the cut's video alone: HEVC Main 10, smaller, one frame for each of the cut's, keyframes where the
   cut has them (every 60 frames on the Osmo Action 6), no B-frames, the parameter sets in each keyframe as the camera
   writes them, `-tune grain` so the noise the denoise is there to show isn't smoothed away by the encoder.
3. The video's samples are swapped into the cut here, natively: ffmpeg and MP4Box can't write DJI's djmd track or
   lose its header (docs/dev/gotchas.md). The video's sample entry stays the camera's (its colr, its btrt...) with the
   new size and the encoder's hvcC; its durations stay the cut's; every other track is copied as it is.
4. The one camera value changed: the lens's focal length in pixels (djmd clip header, ClipMeta.digital_focal_length,
   field 1.8.1, a float). Gyroflow's DJI reader (telemetry-parser src/dji/mod.rs get_lens_profile) takes it with the
   video track's size as the calibration's size, so a 1080p video with the 4K focal length would be a lens twice as
   long. It's scaled by the same factor as the picture (a float in place: no length changes). The distortion
   coefficients (OpenCV fisheye k1..k4) don't depend on the size.
5. The checks: the sample reads as the cut does (gyroflow-probe: colour, stabilization mode, water, lens; the djmd
   samples all equal but the focal length; the frames and their times), see check().

Tools: afp-cut and gyroflow-probe from $AFP_TOOLS, else vs-gyroflow/target/release, else the installed app; ffmpeg and
ffprobe from PATH.
"""
import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONTAINERS = {b'moov', b'trak', b'mdia', b'minf', b'stbl', b'edts', b'dinf'}


# ------------------------------------------------------------------------------------------------ boxes

def parse(b):
    """[kind, body bytes or a list of children] for each box in b; the containers above walked into"""
    out, at = [], 0
    while at + 8 <= len(b):
        size, kind = struct.unpack_from('>I4s', b, at)
        head = 8
        if size == 1:
            size, head = struct.unpack_from('>Q', b, at + 8)[0], 16
        elif size == 0:
            size = len(b) - at
        if size < head or at + size > len(b):
            raise ValueError('a broken box')
        body = b[at + head:at + size]
        out.append([kind, parse(body) if kind in CONTAINERS else bytes(body)])
        at += size
    return out


def write(nodes):
    out = bytearray()
    for kind, body in nodes:
        body = write(body) if isinstance(body, list) else body
        if len(body) + 8 > 0xffffffff:
            out += struct.pack('>I4sQ', 1, kind, len(body) + 16)
        else:
            out += struct.pack('>I4s', len(body) + 8, kind)
        out += body
    return bytes(out)


def kid(nodes, *path):
    for p in path:
        nodes = next(b for k, b in nodes if k == p)
    return nodes


def set_kid(nodes, kind, body):
    for n in nodes:
        if n[0] == kind:
            n[1] = body
            return
    raise KeyError(kind)


def top_level(f):
    f.seek(0, 2)
    n, pos, out = f.tell(), 0, []
    while pos < n:
        f.seek(pos)
        h = f.read(16)
        size, kind = struct.unpack('>I4s', h[:8])
        head = 8
        if size == 1:
            size, head = struct.unpack('>Q', h[8:16])[0], 16
        elif size == 0:
            size = n - pos
        out.append((kind, pos, head, size))
        pos += size
    return out


def read_moov(f):
    top = top_level(f)
    (_, at, head, size), = [t for t in top if t[0] == b'moov']
    f.seek(at + head)
    return top, parse(f.read(size - head))


class Track:
    def __init__(self, trak):
        self.trak = trak
        stbl = kid(trak, b'mdia', b'minf', b'stbl')
        self.stbl = stbl
        self.handler = kid(trak, b'mdia', b'hdlr')[8:12]
        stsd = kid(stbl, b'stsd')
        self.fourcc = stsd[12:16]
        stsz = kid(stbl, b'stsz')
        fixed, n = struct.unpack_from('>II', stsz, 4)
        self.sizes = [fixed] * n if fixed else list(struct.unpack_from(f'>{n}I', stsz, 12))
        if any(k == b'stco' for k, _ in stbl):
            c = kid(stbl, b'stco')
            chunks = struct.unpack_from(f'>{struct.unpack_from(">I", c, 4)[0]}I', c, 8)
        else:
            c = kid(stbl, b'co64')
            chunks = struct.unpack_from(f'>{struct.unpack_from(">I", c, 4)[0]}Q', c, 8)
        stsc = kid(stbl, b'stsc')
        entries = [struct.unpack_from('>III', stsc, 8 + 12 * i)[:2] for i in range(struct.unpack_from('>I', stsc, 4)[0])]
        self.offs = []
        for e, (first, per) in enumerate(entries):
            last = entries[e + 1][0] - 1 if e + 1 < len(entries) else len(chunks)
            for ch in range(first, last + 1):
                o = chunks[ch - 1]
                for _ in range(per):
                    if len(self.offs) == n:
                        break
                    self.offs.append(o)
                    o += self.sizes[len(self.offs) - 1]
        stts = kid(stbl, b'stts')
        self.durs = []
        for i in range(struct.unpack_from('>I', stts, 4)[0]):
            c, d = struct.unpack_from('>II', stts, 8 + 8 * i)
            self.durs += [d] * c
        if len(self.offs) != n or len(self.durs) != n:
            raise ValueError('sample tables that don\'t add up')
        self.sync = None
        if any(k == b'stss' for k, _ in stbl):
            s = kid(stbl, b'stss')
            self.sync = [i - 1 for i in struct.unpack_from(f'>{struct.unpack_from(">I", s, 4)[0]}I', s, 8)]


def tracks_of(moov):
    return [Track(b) for k, b in moov if k == b'trak']


def video_of(tracks):
    vs = [t for t in tracks if t.handler == b'vide' and t.fourcc in (b'hvc1', b'hev1', b'avc1')]
    return max(vs, key=lambda t: len(t.sizes))


def sample(f, t, i):
    f.seek(t.offs[i])
    return f.read(t.sizes[i])


# ------------------------------------------------------------------------------------------------ protobuf

def varint(b, at):
    v = shift = 0
    while True:
        x = b[at]
        at += 1
        v |= (x & 0x7f) << shift
        shift += 7
        if not x & 0x80:
            return v, at


def pb_field(b, num, lo=0, hi=None):
    """(start, end) of the value of the first field `num` between lo and hi: the payload of a length-delimited one,
    the 4 or 8 bytes of a fixed one. None when there's none"""
    hi = len(b) if hi is None else hi
    at = lo
    while at < hi:
        key, at = varint(b, at)
        n, wire = key >> 3, key & 7
        if wire == 0:
            start = at
            _, at = varint(b, at)
        elif wire == 1:
            start, at = at, at + 8
        elif wire == 5:
            start, at = at, at + 4
        elif wire == 2:
            length, start = varint(b, at)
            at = start + length
        else:
            raise ValueError('not protobuf')
        if n == num:
            return start, at
    return None


def pb_path(b, path):
    lo, hi = 0, len(b)
    for num in path:
        r = pb_field(b, num, lo, hi)
        if r is None:
            return None
        lo, hi = r
    return lo, hi


FOCAL = (1, 8, 1)  # ProductMeta.clip_meta.digital_focal_length.focal_length (dvtm_ac206 as telemetry-parser's wm169)


def scale_focal(first_djmd, factor):
    """The DJI clip header with its focal length (px) times factor; (new bytes, old, new focal)"""
    r = pb_path(first_djmd, FOCAL)
    if r is None or r[1] - r[0] != 4:
        raise ValueError('no focal length (1.8.1) in the first djmd sample: not a DJI clip header known here')
    old = struct.unpack('<f', first_djmd[r[0]:r[1]])[0]
    new = old * factor
    return first_djmd[:r[0]] + struct.pack('<f', new) + first_djmd[r[1]:], old, new


# ------------------------------------------------------------------------------------------------ the swap

def entry_with(old_stsd, new_stsd, width, height):
    """The cut's sample description with the new size and the encoder's hvcC (and nothing else of the encoder's)"""
    def split(stsd):
        size = struct.unpack_from('>I', stsd, 8)[0]
        entry = stsd[8:8 + size]
        return entry[:8 + 78], parse(entry[8 + 78:])  # VisualSampleEntry: 8 header + 78 fixed bytes, then boxes
    old_head, old_boxes = split(old_stsd)
    _, new_boxes = split(new_stsd)
    head = bytearray(old_head)
    struct.pack_into('>HH', head, 8 + 24, width, height)
    hvcc = kid(new_boxes, b'hvcC')
    boxes = [[k, hvcc if k == b'hvcC' else b] for k, b in old_boxes]
    body = bytes(head[8:]) + write(boxes)
    entry = struct.pack('>I', len(body) + 8) + bytes(head[4:8]) + body
    return old_stsd[:8] + entry


def swap(cut_path, enc_path, out_path):
    """The cut with the encoded file's video samples, the focal length scaled to the new width. Says what it did"""
    with open(cut_path, 'rb') as fc, open(enc_path, 'rb') as fe:
        top, moov = read_moov(fc)
        _, emoov = read_moov(fe)
        tracks = tracks_of(moov)
        v = video_of(tracks)
        ev = video_of(tracks_of(emoov))
        if v.fourcc != b'hvc1' or ev.fourcc != b'hvc1':
            raise ValueError(f'the video is {v.fourcc}, the encoded one {ev.fourcc}: hvc1 expected for both')
        if len(ev.sizes) != len(v.sizes):
            raise ValueError(f'{len(ev.sizes)} frames encoded for the cut\'s {len(v.sizes)}')
        sync = v.sync if v.sync is not None else list(range(len(v.sizes)))
        esync = ev.sync if ev.sync is not None else list(range(len(ev.sizes)))
        if not set(sync) <= set(esync) and sync != esync:
            raise ValueError('the encoded keyframes aren\'t where the cut\'s are')
        tkhd = kid(v.trak, b'tkhd')
        old_w = struct.unpack_from('>I', tkhd, len(tkhd) - 8)[0] >> 16
        etkhd = kid(ev.trak, b'tkhd')
        w, h = (struct.unpack_from('>I', etkhd, len(etkhd) - 8 + 4 * i)[0] >> 16 for i in (0, 1))
        factor = w / old_w

        # Every sample, in the cut's order in its file, the video's from the encoded file
        order = sorted((t.offs[i], ti, i) for ti, t in enumerate(tracks) for i in range(len(t.sizes)))
        vi = tracks.index(v)
        dj = next((ti for ti, t in enumerate(tracks) if t.fourcc == b'djmd'), None)
        if dj is None:
            raise ValueError('no djmd track: only DJI clips are handled')
        header, old_f, new_f = scale_focal(sample(fc, tracks[dj], 0), factor)

        pre = b''
        post = b''
        first_mdat = next(i for i, t in enumerate(top) if t[0] == b'mdat')
        for i, (kind, at, _, size) in enumerate(top):
            if kind in (b'moov', b'mdat'):
                continue
            fc.seek(at)
            b = fc.read(size)
            if i < first_mdat:
                pre += b
            else:
                post += b

        size_of = lambda ti, i: (ev.sizes[i] if ti == vi else len(header) if (ti, i) == (dj, 0) else tracks[ti].sizes[i])
        total = sum(size_of(ti, i) for _, ti, i in order)
        wide_mdat = total + 8 > 0xffffffff
        mdat_head = struct.pack('>I4sQ', 1, b'mdat', total + 16) if wide_mdat else struct.pack('>I4s', total + 8, b'mdat')
        pos = len(pre) + len(mdat_head)
        offs = [[0] * len(t.sizes) for t in tracks]
        for _, ti, i in order:
            offs[ti][i] = pos
            pos += size_of(ti, i)
        wide = pos > 0xffffffff

        # The tables
        for t, o in zip(tracks, offs):
            stbl = t.stbl
            kinds = [k for k, _ in stbl]
            co = b'co64' if wide else b'stco'
            body = struct.pack('>II', 0, len(o)) + b''.join(struct.pack('>Q' if wide else '>I', x) for x in o)
            for n in stbl:
                if n[0] in (b'stco', b'co64'):
                    n[0], n[1] = co, body
            set_kid(stbl, b'stsc', struct.pack('>IIIII', 0, 1, 1, 1, 1))
            if t is v:
                set_kid(stbl, b'stsd', entry_with(kid(stbl, b'stsd'), kid(ev.stbl, b'stsd'), w, h))
                set_kid(stbl, b'stsz', struct.pack('>III', 0, 0, len(ev.sizes)) + struct.pack(f'>{len(ev.sizes)}I', *ev.sizes))
                if ev.sync is not None:
                    stss = struct.pack('>II', 0, len(ev.sync)) + struct.pack(f'>{len(ev.sync)}I', *[i + 1 for i in ev.sync])
                    if b'stss' in kinds:
                        set_kid(stbl, b'stss', stss)
                    else:
                        stbl.insert(kinds.index(b'stsz'), [b'stss', stss])
                elif b'stss' in kinds:
                    stbl[:] = [n for n in stbl if n[0] != b'stss']
                # What the camera said of its own frames' dependencies isn't true of the encoder's
                stbl[:] = [n for n in stbl if n[0] not in (b'sdtp', b'sgpd', b'sbgp', b'ctts')]
            if dj is not None and t is tracks[dj]:
                sizes = list(t.sizes)
                sizes[0] = len(header)
                set_kid(stbl, b'stsz', struct.pack('>III', 0, 0, len(sizes)) + struct.pack(f'>{len(sizes)}I', *sizes))
        tk = bytearray(tkhd)
        struct.pack_into('>II', tk, len(tk) - 8, w << 16, h << 16)
        set_kid(v.trak, b'tkhd', bytes(tk))

        with open(out_path, 'wb') as out:
            out.write(pre)
            out.write(mdat_head)
            for _, ti, i in order:
                if ti == vi:
                    out.write(sample(fe, ev, i))
                elif (ti, i) == (dj, 0):
                    out.write(header)
                else:
                    out.write(sample(fc, tracks[ti], i))
            out.write(write([[b'moov', moov]]))
            out.write(post)
    return {'size': [w, h], 'from_width': old_w, 'frames': len(v.sizes), 'keyframes': len(esync),
            'focal_px': [round(old_f, 3), round(new_f, 3)]}


# ------------------------------------------------------------------------------------------------ tools

def tool(name):
    for d in (os.environ.get('AFP_TOOLS'), os.path.join(ROOT, 'vs-gyroflow', 'target', 'release'),
              os.path.expanduser('~/Applications/ActionFootagePlayer.app/Contents/Resources/plugins/gyroflow'),
              '/Applications/ActionFootagePlayer.app/Contents/Resources/plugins/gyroflow'):
        if d and os.access(os.path.join(d, name), os.X_OK):
            return os.path.join(d, name)
    sys.exit(f'{name} not found: build vs-gyroflow, install the app, or set AFP_TOOLS')


def probe(path, *flags):
    r = subprocess.run([tool('gyroflow-probe'), *flags, path], capture_output=True, text=True)
    return json.loads(r.stdout) if r.stdout.strip().startswith('{') else {'error': r.stdout + r.stderr}


def frames(path):
    r = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries',
                        'packet=pts_time,flags', '-of', 'csv=p=0', path], capture_output=True, text=True, check=True)
    return [line.split(',') for line in r.stdout.split()]


def depth(path):
    """(in water, depth m) per djmd sample: FrameMeta Barometer 3.2.13 {1 in water, 2 altitude dm}"""
    out = []
    with open(path, 'rb') as f:
        _, moov = read_moov(f)
        t = next(t for t in tracks_of(moov) if t.fourcc == b'djmd')
        for i in range(len(t.sizes)):
            s = sample(f, t, i)
            r = pb_path(s, (3, 2, 13))
            if r is None:
                out.append(None)
                continue
            m = s[r[0]:r[1]]
            w = pb_field(m, 1)
            a = pb_field(m, 2)
            alt = varint(m, a[0])[0] if a else None
            if alt is not None and alt >= 1 << 63:
                alt -= 1 << 64
            out.append((varint(m, w[0])[0] if w else 0, alt / 10 if alt is not None else None))
    return out


def djmd_samples(path):
    with open(path, 'rb') as f:
        _, moov = read_moov(f)
        t = next(t for t in tracks_of(moov) if t.fourcc == b'djmd')
        return [sample(f, t, i) for i in range(len(t.sizes))]


def check(cut, out):
    """What must read the same in the sample as in the cut; a list of problems (empty: it passed) and what was read"""
    problems = []
    a, b = probe(cut, '--library'), probe(out, '--library')
    for k in ('mode', 'stabilized_in_camera', 'color', 'water', 'rotation', 'camera'):
        if a.get(k) != b.get(k):
            problems.append(f'probe {k}: {a.get(k)!r} in the cut, {b.get(k)!r} in the sample')
    la, lb = a.get('lens_info') or {}, b.get('lens_info') or {}
    for k in ('lens', 'recorded', 'from', 'mode'):
        if la.get(k) != lb.get(k):
            problems.append(f'lens {k}: {la.get(k)!r} in the cut, {lb.get(k)!r} in the sample')
    if abs((a.get('fps') or 0) - (b.get('fps') or 0)) > 1e-6:
        problems.append(f'fps {a.get("fps")} → {b.get("fps")}')
    fa, fb = frames(cut), frames(out)
    if [x[0] for x in fa] != [x[0] for x in fb]:
        problems.append(f'the frames\' times differ ({len(fa)} → {len(fb)} frames)')
    if [i for i, x in enumerate(fa) if 'K' in x[1]] != [i for i, x in enumerate(fb) if 'K' in x[1]]:
        problems.append('the keyframes moved')
    sa, sb = djmd_samples(cut), djmd_samples(out)
    if len(sa) != len(sb) or sa[1:] != sb[1:]:
        problems.append('djmd samples differ')
    elif pb_path(sa[0], FOCAL) is None or (sa[0][:pb_path(sa[0], FOCAL)[0]], sa[0][pb_path(sa[0], FOCAL)[1]:]) != \
            (sb[0][:pb_path(sb[0], FOCAL)[0]], sb[0][pb_path(sb[0], FOCAL)[1]:]):
        problems.append('the djmd clip header differs in more than its focal length')
    d = [x for x in depth(out) if x and x[0] and x[1] is not None]
    return problems, {
        'probe': {k: b.get(k) for k in ('mode', 'width', 'height', 'fps', 'stabilized_in_camera', 'color', 'water', 'camera')},
        'lens': {k: lb.get(k) for k in ('lens', 'recorded', 'from', 'mode_key')},
        'frames': len(fb), 'keyframes': sum('K' in x[1] for x in fb),
        'depth_m': [min(x[1] for x in d), max(x[1] for x in d)] if d else None,
        'in_water': f'{len(d)}/{len(sb)}',
    }


def encode(cut, out, height, crf, preset):
    w = round(height * 16 / 9 / 2) * 2
    x265 = ':'.join(['log-level=error', 'bframes=0', 'keyint=60', 'min-keyint=60', 'scenecut=0', 'open-gop=0',
                     'colorprim=bt709', 'transfer=bt709', 'colormatrix=bt709', 'range=limited', 'repeat-headers=1'])
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', cut, '-map', '0:v:0',
                    '-vf', f'scale={w}:{height}:flags=lanczos:out_range=tv', '-c:v', 'libx265', '-preset', preset,
                    '-tune', 'grain', '-crf', str(crf), '-pix_fmt', 'yuv420p10le', '-x265-params', x265,
                    '-tag:v', 'hvc1', '-an', '-fps_mode', 'passthrough', out], check=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0], formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('clip', nargs='?')
    ap.add_argument('--from', dest='start', type=float)
    ap.add_argument('--to', dest='end', type=float)
    ap.add_argument('--out', required=False)
    ap.add_argument('--height', type=int, default=1080)
    ap.add_argument('--crf', type=float, default=24)
    ap.add_argument('--preset', default='slow')
    ap.add_argument('--swap', nargs=2, metavar=('CUT', 'ENCODED'))
    ap.add_argument('--check', nargs=2, metavar=('CUT', 'SAMPLE'))
    ap.add_argument('--keep', help='a folder to keep the cut and the encoded video in')
    a = ap.parse_args()

    if a.check:
        problems, read = check(*a.check)
        print(json.dumps({'read': read, 'problems': problems}, indent=1))
        return 1 if problems else 0
    if a.swap:
        print(json.dumps(swap(*a.swap, a.out)))
        problems, read = check(a.swap[0], a.out)
        print(json.dumps({'read': read, 'problems': problems}, indent=1))
        return 1 if problems else 0
    if not (a.clip and a.start is not None and a.out):
        ap.error('a clip, --from and --out are needed')
    work = a.keep or tempfile.mkdtemp(prefix='afp-sample-')
    os.makedirs(work, exist_ok=True)
    cut, enc = os.path.join(work, 'cut.mp4'), os.path.join(work, 'video.mp4')
    args = [tool('afp-cut'), a.clip, '--from', str(a.start), '--out', cut, '--json']
    if a.end is not None:
        args += ['--to', str(a.end)]
    if os.path.exists(cut):
        os.remove(cut)
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f'afp-cut: {r.stdout}{r.stderr}')
    encode(cut, enc, a.height, a.crf, a.preset)
    done = swap(cut, enc, a.out)
    problems, read = check(cut, a.out)
    done['bytes'] = os.path.getsize(a.out)
    print(json.dumps({'made': done, 'read': read, 'problems': problems}, indent=1))
    if not a.keep:
        shutil.rmtree(work)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
