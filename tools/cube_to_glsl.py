#!/usr/bin/env python3
"""Bake a .cube 3D LUT into an mpv user shader.

    cube_to_glsl.py <template.glsl.in> <lut.cube> <out.glsl>

The template's `@LUT_SIZE@` becomes the LUT's edge size and `@LUT_DATA@` the LUT as an inline `rgba16` texture:
one line of hex, 4 little-endian unorm16 per texel (alpha 0xffff), red fastest, the .cube's own order. mpv wants
the hex on a single line, and `rgba16f` would want 16 bytes per texel, hence unorm16.
"""
import struct
import sys


def read_cube(path):
    size, values = None, []
    with open(path) as f:
        for line in f:
            t = line.split()
            if not t or t[0].startswith('#'):
                continue
            if t[0] == 'LUT_3D_SIZE':
                size = int(t[1])
            elif t[0][0].isalpha():
                continue  # TITLE, DOMAIN_MIN/MAX (0..1 assumed)
            else:
                values.append(tuple(float(x) for x in t[:3]))
    if size is None or len(values) != size ** 3:
        sys.exit(f'{path}: not a 3D .cube LUT ({len(values)} entries, LUT_3D_SIZE {size})')
    return size, values


def texture_hex(values):
    unorm = lambda v: max(0, min(65535, int(round(v * 65535))))
    return b''.join(struct.pack('<4H', unorm(r), unorm(g), unorm(b), 65535) for r, g, b in values).hex()


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    template, cube, out = sys.argv[1:]
    size, values = read_cube(cube)
    with open(template, newline='') as f:
        text = f.read()
    text = text.replace('@LUT_SIZE@', str(size)).replace('@LUT_DATA@', texture_hex(values))
    with open(out, 'w', newline='') as f:
        f.write(text)
    print(f'{out}: {size}³ LUT from {cube}')


if __name__ == '__main__':
    main()
