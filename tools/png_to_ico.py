#!/usr/bin/env python3
"""tools/png_to_ico.py <out.ico> <png>...: packs square PNGs into a Windows icon, each image as PNG (Windows Vista
and later read them so), for the Windows app (tools/make_icon.sh makes the PNGs from the SVG). Sizes up to 256."""
import struct
import sys


def png_size(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("not a PNG")
    return struct.unpack(">II", data[16:24])


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    images = []
    for path in sys.argv[2:]:
        with open(path, "rb") as f:
            data = f.read()
        w, h = png_size(data)
        if w != h or w > 256:
            sys.exit(f"{path}: {w}x{h}, an icon's images are square, up to 256")
        images.append((w, data))
    images.sort(key=lambda i: i[0])
    # ICONDIR, then an ICONDIRENTRY per image (0 stands for 256), then the images
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in images:
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(data for _, data in images)
    with open(sys.argv[1], "wb") as f:
        f.write(out)


if __name__ == "__main__":
    main()
