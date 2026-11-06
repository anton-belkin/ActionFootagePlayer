#!/usr/bin/env python3
"""tests/make_equirect.py <out-dir> [seconds]: synthetic stitched 360° clips for the view360 script, until real ones

  equirect-v2.mp4   an equirectangular latitude/longitude grid with Spherical Video V2 metadata (sv3d, st3d)
  equirect-v1.mp4   the same with V1's (the XMP in trak/uuid), looking 30° right first (InitialViewHeadingDegrees)
  equirect-flat.mp4 the same picture without any: an ordinary clip, which must play as it is

The grid has a line every 15°, the equator and the meridian ahead thicker, each quarter its own color (ahead red,
right green, behind blue, left yellow) and labelled with words that read wrong when mirrored, so a turned or mirrored
view is plain to see. A white box goes round the equator to the right, once every 8 s, to tell playing from paused.
The conventions are video-types' (docs/360-cameras.md): longitude 0 in the picture's middle, +90° (right) at three
quarters of its width, latitude +90° (up) at its top.

Needs ffmpeg (libx264) and Pillow. 2880 x 1440, H.264, 30 fps: a few MB.
"""
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

W, H = 2880, 1440
FONTS = ["/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "C:/Windows/Fonts/arial.ttf"]


def font(size):
    for f in FONTS:
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def px(lon, lat):
    return (0.5 + lon / 360) * W, (0.5 - lat / 180) * H


def grid():
    img = Image.new("RGB", (W, H))
    d = ImageDraw.Draw(img)
    # The quarters: ahead (-45..45) red, right green, behind blue, left yellow
    for lon0, color in ((-45, (110, 30, 30)), (45, (30, 100, 40)), (135, (30, 40, 110)), (-135, (110, 100, 20))):
        for lo in (lon0, lon0 - 360, lon0 + 360):
            x0, _ = px(lo, 0)
            x1, _ = px(lo + 90, 0)
            d.rectangle([x0, 0, x1, H], fill=color)
    for lon in range(-180, 181, 15):
        x = px(lon, 0)[0]
        d.line([x, 0, x, H], fill=(230, 230, 230) if lon % 90 == 0 else (150, 150, 150), width=6 if lon == 0 else (3 if lon % 90 == 0 else 1))
    for lat in range(-75, 76, 15):
        y = px(0, lat)[1]
        d.line([0, y, W, y], fill=(230, 230, 230) if lat == 0 else (150, 150, 150), width=6 if lat == 0 else 1)
    big, mid, small = font(90), font(48), font(30)
    for lon, word in ((0, "AHEAD 0°"), (90, "RIGHT +90°"), (180, "BEHIND 180°"), (-90, "LEFT -90°")):
        for lo in ((lon, lon - 360) if lon == 180 else (lon,)):
            x, y = px(lo, 0)
            d.text((x, y - 70), word, font=big, fill="white", anchor="ms")
    for lat in (-60, -30, 30, 60):
        for lon in (0, 90, -90, 180):
            x, y = px(lon, lat)
            d.text((x + 8, y - 8), f"{lat:+d}°", font=small, fill="white", anchor="ls")
    x, y = px(0, 75)
    d.text((x, y), "SKY UP", font=mid, fill="white", anchor="mm")
    x, y = px(0, -75)
    d.text((x, y), "GROUND DOWN", font=mid, fill="white", anchor="mm")
    # Ahead, up and to the right: an arrow pointing up and right, whose turn shows a roll
    x, y = px(30, 35)
    d.polygon([(x, y), (x - 60, y + 10), (x - 10, y + 60)], fill="white")
    return img


def box(kind, payload):
    return struct.pack(">I", 8 + len(payload)) + kind + payload


def boxes(data, start, end):
    pos = start
    while pos + 8 <= end:
        size, kind = struct.unpack(">I4s", data[pos:pos + 8])
        header = 8
        if size == 1:
            size, header = struct.unpack(">Q", data[pos + 8:pos + 16])[0], 16
        elif size == 0:
            size = end - pos
        yield kind, pos, header, size
        pos += size


def inject(path, out, entry_boxes=b"", trak_boxes=b""):
    """The boxes added at the end of the first video sample entry and of its trak, each parent's size grown to match.
    The moov must be at the file's end (ffmpeg's default): nothing after it moves"""
    data = bytearray(Path(path).read_bytes())
    def find(start, end, kinds):
        for kind, pos, header, size in boxes(data, start, end):
            if kind == kinds[0]:
                if len(kinds) == 1:
                    return [(pos, header, size)]
                rest = find(pos + header, pos + size, kinds[1:])
                if rest:
                    return [(pos, header, size)] + rest
        return None

    moov = find(0, len(data), [b"moov"])
    assert moov, "no moov"
    mpos, mh, msize = moov[0]
    assert mpos + msize == len(data), "moov isn't at the end"
    trak = None
    for kind, pos, header, size in boxes(data, mpos + mh, mpos + msize):
        if kind != b"trak":
            continue
        hdlr = find(pos + header, pos + size, [b"mdia", b"hdlr"])
        if hdlr and data[hdlr[-1][0] + 16:hdlr[-1][0] + 20] == b"vide":
            trak = (pos, header, size)
            break
    assert trak, "no video track"
    stsd = find(trak[0] + trak[1], trak[0] + trak[2], [b"mdia", b"minf", b"stbl", b"stsd"])
    spos, sh, ssize = stsd[-1]
    # The first sample entry, after stsd's version, flags and count
    epos = spos + sh + 8
    esize = struct.unpack(">I", data[epos:epos + 4])[0]
    chain = [moov[0], trak] + stsd + [(epos, 8, esize)]
    inserts = [(epos + esize, entry_boxes, chain), (trak[0] + trak[2], trak_boxes, [moov[0], trak])]
    # From the end backwards, so the positions found stay right
    for at, payload, parents in sorted(inserts, key=lambda i: -i[0]):
        if not payload:
            continue
        data[at:at] = payload
        for ppos, _, _ in parents:
            size = struct.unpack(">I", data[ppos:ppos + 4])[0]
            assert size != 1, "64-bit box sizes aren't handled"
            data[ppos:ppos + 4] = struct.pack(">I", size + len(payload))
    Path(out).write_bytes(data)


def v2_boxes():
    st3d = box(b"st3d", b"\0\0\0\0" + bytes([0]))
    svhd = box(b"svhd", b"\0\0\0\0" + b"ActionFootagePlayer tests\0")
    prhd = box(b"prhd", b"\0\0\0\0" + struct.pack(">iii", 0, 0, 0))
    equi = box(b"equi", b"\0\0\0\0" + struct.pack(">IIII", 0, 0, 0, 0))
    sv3d = box(b"sv3d", svhd + box(b"proj", prhd + equi))
    return st3d + sv3d


def v1_box():
    xmp = ('<?xml version="1.0"?><rdf:SphericalVideo xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" '
           'xmlns:GSpherical="http://ns.google.com/videos/1.0/spherical/">'
           '<GSpherical:Spherical>true</GSpherical:Spherical><GSpherical:Stitched>true</GSpherical:Stitched>'
           '<GSpherical:StitchingSoftware>ActionFootagePlayer tests</GSpherical:StitchingSoftware>'
           '<GSpherical:ProjectionType>equirectangular</GSpherical:ProjectionType>'
           '<GSpherical:InitialViewHeadingDegrees>30</GSpherical:InitialViewHeadingDegrees>'
           '</rdf:SphericalVideo>').encode()
    uuid = bytes.fromhex("ffcc8263f8554a938814587a02521fdd")
    return box(b"uuid", uuid + xmp)


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit("usage: tests/make_equirect.py <out-dir> [seconds]")
    out = Path(sys.argv[1])
    seconds = float(sys.argv[2]) if len(sys.argv) == 3 else 8
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "grid.png"
        grid().save(png)
        flat = out / "equirect-flat.mp4"
        # The box round the equator, to the right: once every 8 s
        x = f"mod(t*{W}/8+{W // 2}-30\\,{W})"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-loop", "1", "-framerate", "30", "-i", str(png), "-t", str(seconds),
                        "-vf", f"drawbox=x={x}:y={H // 2 - 30}:w=60:h=60:color=white:t=fill,format=yuv420p",
                        "-c:v", "libx264", "-crf", "24", "-preset", "veryfast", "-g", "30", str(flat)], check=True)
        inject(flat, out / "equirect-v2.mp4", entry_boxes=v2_boxes())
        inject(flat, out / "equirect-v1.mp4", trak_boxes=v1_box())
    for f in ("equirect-v2.mp4", "equirect-v1.mp4", "equirect-flat.mp4"):
        print(out / f)


if __name__ == "__main__":
    main()
