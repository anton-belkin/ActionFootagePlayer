#!/usr/bin/env python3
"""tools/make_sample_clip.py: the DJI header's focal length found and scaled in place, nothing else of the header
changed; the video's sample entry keeps the camera's boxes with the new size and the encoder's hvcC."""
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools'))
import make_sample_clip as m  # noqa: E402


def varint(v):
    out = bytearray()
    while True:
        b = v & 0x7f
        v >>= 7
        out.append(b | (0x80 if v else 0))
        if not v:
            return bytes(out)


def field(num, wire, payload):
    key = varint(num << 3 | wire)
    return key + (varint(len(payload)) + payload if wire == 2 else payload)


def test_focal():
    # ProductMeta { clip_meta(1) { header(1) "dvtm_ac206.proto", distortion(3) packed floats, eis(9) {1: 0},
    #   focal(8) { focal_length(1) float } }, stream_meta(2) { color(4) {1: 19} } }
    dist = field(1, 2, struct.pack('<4f', 0.15, 0.13, -0.09, 0.004))
    clip = (field(1, 2, field(1, 2, b'dvtm_ac206.proto')) + field(3, 2, dist) + field(8, 2, field(1, 5, struct.pack('<f', 1473.121)))
            + field(9, 2, field(1, 0, varint(0))))
    header = field(1, 2, clip) + field(2, 2, field(4, 2, field(1, 0, varint(19))))
    new, old, now = m.scale_focal(header, 0.5)
    assert abs(old - 1473.121) < 1e-3 and abs(now - 736.5605) < 1e-3, (old, now)
    assert len(new) == len(header)
    r = m.pb_path(new, m.FOCAL)
    assert abs(struct.unpack('<f', new[r[0]:r[1]])[0] - 736.5605) < 1e-3
    assert new[:r[0]] == header[:r[0]] and new[r[1]:] == header[r[1]:], 'only the focal length changes'
    # A header without it is refused, never passed through with the 4K lens
    try:
        m.scale_focal(field(1, 2, field(3, 2, dist)), 0.5)
        raise AssertionError('a header without a focal length was taken')
    except ValueError:
        pass


def entry(w, h, boxes):
    body = bytes(6) + struct.pack('>H', 1) + bytes(16) + struct.pack('>HH', w, h) + bytes(78 - 28) + m.write(boxes)
    e = struct.pack('>I', len(body) + 8) + b'hvc1' + body
    return struct.pack('>II', 0, 1) + e


def test_entry():
    camera = entry(3840, 2160, [[b'hvcC', b'4K'], [b'colr', b'nclx-camera'], [b'btrt', b'rates']])
    encoder = entry(1920, 1080, [[b'hvcC', b'1080p'], [b'colr', b'nclx-encoder'], [b'pasp', b'11']])
    out = m.entry_with(camera, encoder, 1920, 1080)
    assert struct.unpack_from('>HH', out, 8 + 32) == (1920, 1080)
    boxes = m.parse(out[8 + 86:])
    assert boxes == [[b'hvcC', b'1080p'], [b'colr', b'nclx-camera'], [b'btrt', b'rates']], boxes
    assert struct.unpack_from('>I', out, 8)[0] == len(out) - 8


if __name__ == '__main__':
    test_focal()
    test_entry()
    print('make_sample_clip: ok')
