#!/usr/bin/env python3
"""Look inside the telemetry DJI cameras record: the protobuf samples of the file's metadata track, one per frame.

    dji_meta_dump.py summary  <clip.MP4>                 every field: how often, min, max, first, last
    dji_meta_dump.py timeline <clip.MP4> <path>... [-s 1]  the values of some fields over time, every -s seconds
    dji_meta_dump.py extract  <clip.MP4> [-o out] [--start S] [--duration D]   the raw samples, to send around

Fields are named by their path of field numbers (`3.2.18.1`: field 1 of field 18 of field 2 of field 3), since the
schemas of newer models (Osmo Action 6: dvtm_ac206.proto) aren't public. 32-bit and 64-bit fields are shown as
floats, varints as integers, short byte strings in hex. Without a schema a packed repeated field is ambiguous: its
bytes sometimes parse as a message too, so 3.2.15.1 (an aperture, [28, 10] = f/2.8) shows up as 3.2.15.1.4 when it
is [32, 10]. `summary` and `timeline` read an .MP4 or an `extract` output.

The track is the one the dlogm plugin reads (mpv/scripts/dlogm/dji_color.lua): the first with a `meta` handler.
"""
import argparse
import gzip
import math
import os
import struct
import sys

MAGIC = b'DJIMETA1'


def boxes(s, first, last):
    pos = first
    while pos + 8 <= last:
        size, typ, header = struct.unpack_from('>I', s, pos)[0], s[pos + 4:pos + 8], 8
        if size == 1:
            size, header = struct.unpack_from('>Q', s, pos + 8)[0], 16
        elif size == 0:
            size = last - pos
        if size < header or pos + size > last:
            return
        yield typ, pos + header, pos + size
        pos += size


def child(s, first, last, wanted):
    for typ, a, b in boxes(s, first, last):
        if typ == wanted:
            return a, b
    return None, None


def read_moov(f):
    f.seek(0, os.SEEK_END)
    file_size, pos = f.tell(), 0
    while pos + 8 <= file_size:
        f.seek(pos)
        head = f.read(16)
        size, typ, header = struct.unpack_from('>I', head)[0], head[4:8], 8
        if size == 1:
            size, header = struct.unpack_from('>Q', head, 8)[0], 16
        elif size == 0:
            size = file_size - pos
        if size < header:
            break
        if typ == b'moov':
            f.seek(pos)
            return f.read(size)
        pos += size
    sys.exit('no moov box: not an MP4/MOV file?')


def metadata_track(moov):
    """(timescale, [(time in seconds, offset, size)]) of the first track with a `meta` handler"""
    for typ, ta, tb in boxes(moov, 8, len(moov)):
        if typ != b'trak':
            continue
        ma, mb = child(moov, ta, tb, b'mdia')
        ha, _ = child(moov, ma, mb, b'hdlr') if ma else (None, None)
        if ha is None or moov[ha + 8:ha + 12] != b'meta':
            continue
        da, _ = child(moov, ma, mb, b'mdhd')
        version = moov[da]
        timescale = struct.unpack_from('>I', moov, da + (20 if version == 1 else 12))[0]
        ia, ib = child(moov, ma, mb, b'minf')
        sa, sb = child(moov, ia, ib, b'stbl')
        return timescale, sample_table(moov, sa, sb, timescale)
    sys.exit('no metadata track: not a DJI camera file?')


def sample_table(s, sa, sb, timescale):
    a, _ = child(s, sa, sb, b'stsz')
    fixed, count = struct.unpack_from('>II', s, a + 4)
    sizes = [fixed] * count if fixed else list(struct.unpack_from(f'>{count}I', s, a + 12))

    a, _ = child(s, sa, sb, b'stco')
    if a is not None:
        n = struct.unpack_from('>I', s, a + 4)[0]
        chunks = struct.unpack_from(f'>{n}I', s, a + 8)
    else:
        a, _ = child(s, sa, sb, b'co64')
        n = struct.unpack_from('>I', s, a + 4)[0]
        chunks = struct.unpack_from(f'>{n}Q', s, a + 8)

    a, _ = child(s, sa, sb, b'stsc')
    n = struct.unpack_from('>I', s, a + 4)[0]
    stsc = [struct.unpack_from('>III', s, a + 8 + 12 * k) for k in range(n)]

    a, _ = child(s, sa, sb, b'stts')
    n = struct.unpack_from('>I', s, a + 4)[0]
    times, t = [], 0
    for k in range(n):
        c, d = struct.unpack_from('>II', s, a + 8 + 8 * k)
        for _ in range(c):
            times.append(t / timescale)
            t += d

    samples, i = [], 0
    for k, (first, per_chunk, _) in enumerate(stsc):
        last = stsc[k + 1][0] - 1 if k + 1 < len(stsc) else len(chunks)
        for chunk in range(first - 1, last):
            offset = chunks[chunk]
            for _ in range(per_chunk):
                if i >= count:
                    break
                samples.append((times[i] if i < len(times) else math.nan, offset, sizes[i]))
                offset += sizes[i]
                i += 1
    return samples


def read_samples(path):
    """[(time in seconds, bytes)] from an .MP4 or an `extract` output"""
    with open(path, 'rb') as f:
        if f.read(2) == b'\x1f\x8b':
            f.seek(0)
            data = gzip.decompress(f.read())
            if not data.startswith(MAGIC):
                sys.exit(f'{path}: not an extract output')
            out, pos = [], len(MAGIC)
            while pos < len(data):
                t, n = struct.unpack_from('<dI', data, pos)
                pos += 12
                out.append((t, data[pos:pos + n]))
                pos += n
            return out
        moov = read_moov(f)
        _, table = metadata_track(moov)
        out = []
        for t, offset, size in table:
            f.seek(offset)
            out.append((t, f.read(size)))
        return out


# Protobuf without a schema: a length-delimited field is a message when it parses as one to the last byte

def varint(s, i):
    v, shift = 0, 0
    while i < len(s):
        b = s[i]
        i += 1
        v |= (b & 0x7f) << shift
        if b < 0x80:
            return v, i
        shift += 7
        if shift > 63:
            return None, i
    return None, i


def parse(s):
    """[(field, wire, value)] or None when s isn't a well-formed message"""
    out, i = [], 0
    while i < len(s):
        key, i = varint(s, i)
        if key is None or key >> 3 == 0:
            return None
        field, wire = key >> 3, key & 7
        if wire == 0:
            v, i = varint(s, i)
            if v is None:
                return None
            out.append((field, 'int', v - (1 << 64) if v >= 1 << 63 else v))
        elif wire == 1 or wire == 5:
            n = 8 if wire == 1 else 4
            if i + n > len(s):
                return None
            out.append((field, 'f64' if wire == 1 else 'f32', struct.unpack_from('<d' if wire == 1 else '<f', s, i)[0]))
            i += n
        elif wire == 2:
            n, i = varint(s, i)
            if n is None or i + n > len(s):
                return None
            out.append((field, 'bytes', s[i:i + n]))
            i += n
        else:
            return None
    return out


def walk(s, prefix=''):
    """Every leaf of a message: (path, kind, value)"""
    for field, kind, v in parse(s) or []:
        path = f'{prefix}{field}'
        if kind == 'bytes':
            sub = parse(v) if v else None
            if sub:
                yield from walk(v, path + '.')
                continue
            try:
                text = v.decode('utf-8')
                if text.isprintable():
                    yield path, 'str', text
                    continue
            except UnicodeDecodeError:
                pass
            yield path, 'bytes', v
        else:
            yield path, kind, v


def path_key(p):
    return [int(x) for x in p.split('.')]


def fmt(v):
    if isinstance(v, float):
        return f'{v:.6g}'
    if isinstance(v, bytes):
        return v.hex() if len(v) <= 6 else f'<{len(v)} bytes>'
    if isinstance(v, str):
        return repr(v if len(v) <= 40 else v[:37] + '...')
    return str(v)


def summary(samples):
    stats = {}
    for t, s in samples:
        seen = {}
        for path, kind, v in walk(s):
            seen[path] = seen.get(path, 0) + 1
            st = stats.setdefault(path, {'kind': kind, 'frames': 0, 'count': 0, 'min': v, 'max': v,
                                         'first': v, 'last': v, 'values': set()})
            st['count'] += 1
            st['last'] = v
            if isinstance(v, (int, float)) and not (isinstance(v, float) and math.isnan(v)):
                if not isinstance(st['min'], (int, float)) or v < st['min']:
                    st['min'] = v
                if not isinstance(st['max'], (int, float)) or v > st['max']:
                    st['max'] = v
            if len(st['values']) < 1000:
                st['values'].add(v if not isinstance(v, bytes) else bytes(v[:64]))
        for path in seen:
            stats[path]['frames'] += 1

    duration = samples[-1][0] - samples[0][0] if len(samples) > 1 else 0
    print(f'{len(samples)} samples over {duration:.2f} s, {sum(len(s) for _, s in samples) / max(len(samples), 1):.0f} '
          f'bytes each on average')
    for path, kind, v in walk(samples[0][1]):
        if kind == 'str':
            print(f'  {path} = {fmt(v)}')
    print()
    print(f'{"path":<16} {"type":<5} {"frames":>6} {"/frame":>6} {"distinct":>8}  '
          f'{"min":>12} {"max":>12} {"first":>12} {"last":>12}')
    for path in sorted(stats, key=path_key):
        st = stats[path]
        distinct = len(st['values'])
        print(f'{path:<16} {st["kind"]:<5} {st["frames"]:>6} {st["count"] / max(st["frames"], 1):>6.1f} '
              f'{(str(distinct) + ("+" if distinct >= 1000 else "")):>8}  {fmt(st["min"]):>12} {fmt(st["max"]):>12} '
              f'{fmt(st["first"]):>12} {fmt(st["last"]):>12}')


def timeline(samples, paths, step):
    print('time      ' + ' '.join(f'{p:>14}' for p in paths))
    next_t = -math.inf
    for t, s in samples:
        if t < next_t:
            continue
        next_t = t + step
        values = {}
        for path, _, v in walk(s):
            if path in paths and path not in values:
                values[path] = v
        print(f'{t:8.2f}  ' + ' '.join(f'{fmt(values[p]) if p in values else "-":>14}' for p in paths))


def extract(samples, out, start, duration):
    data = bytearray(MAGIC)
    n = 0
    for t, s in samples:
        if t < start or (duration is not None and t >= start + duration):
            continue
        data += struct.pack('<dI', t, len(s)) + s
        n += 1
    with open(out, 'wb') as f:
        f.write(gzip.compress(bytes(data), 9))
    print(f'{out}: {n} samples, {os.path.getsize(out) / 1e6:.1f} MB')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('summary')
    p.add_argument('file')
    p = sub.add_parser('timeline')
    p.add_argument('file')
    p.add_argument('paths', nargs='+')
    p.add_argument('-s', '--step', type=float, default=1.0, help='seconds between rows (default 1)')
    p = sub.add_parser('extract')
    p.add_argument('file')
    p.add_argument('-o', '--out', help='default: <clip>.djimeta.gz')
    p.add_argument('--start', type=float, default=0.0)
    p.add_argument('--duration', type=float)
    a = ap.parse_args()

    samples = read_samples(a.file)
    if not samples:
        sys.exit('the metadata track is empty')
    if a.cmd == 'summary':
        summary(samples)
    elif a.cmd == 'timeline':
        timeline(samples, a.paths, a.step)
    else:
        extract(samples, a.out or os.path.splitext(a.file)[0] + '.djimeta.gz', a.start, a.duration)


if __name__ == '__main__':
    main()
