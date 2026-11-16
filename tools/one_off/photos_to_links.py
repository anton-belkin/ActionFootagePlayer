#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""One-off: the photos rendered before photos were links (a full HEIC or JPEG with afp:link in its XMP, a Live
Photo's HEIC + MOV) turned into photo objects (docs/albums-photos-share-concept.md, "Photos made before this").

NOT a feature: run once, by hand, on Anton's library, when he says so. Written 2026-11-16, never run on real data.

For each photo in the project's folder that carries a link and isn't a link file already:
  1. a photo object is made where the file is (same folder, same name): <name>.afplink.jpg, a small JPEG (1280 px,
     by macOS's sips) with the photo's own link in its XMP (clip, time; a crop isn't in old links);
  2. with --move-renders, the file (a Live Photo's .mov with it) moves to _Renders/Photos/<its path in the project>.
     WAIT with this until the render cache says where photo renders live (found by recipe hash): without the flag
     the full file stays beside its link, and the Library shows both.

It lists what it will do and does nothing without --go. It deletes nothing and overwrites nothing.

  python3 tools/one_off/photos_to_links.py <project folder> [--move-renders] [--go]
"""
import os, re, struct, subprocess, sys, tempfile, html

XMP_HEAD = b"http://ns.adobe.com/xap/1.0/\0"
SKIP_DIRS = {"_renders", "_cache", ".afp"}


def read_link(path):
    """The afp:link of a photo, as its XML-unescaped JSON text: from the XMP in its first 64 KiB."""
    with open(path, "rb") as f:
        s = f.read(65536).decode("utf-8", "replace")
    a = s.find("<x:xmpmeta")
    if a < 0:
        return None
    m = re.search(r"<afp:link>(.*?)</afp:link>", s, re.S) or re.search(r'afp:link="([^"]*)"', s)
    return html.unescape(m.group(1)) if m else None


def with_link(jpeg, link):
    """A JPEG with the link in an XMP packet of its own (as library/src/albums.rs photo_file writes it)."""
    assert jpeg[:2] == b"\xff\xd8"
    packet = ('<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/">'
              '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" '
              'xmlns:afp="https://actionfootageplayer.ch/ns/1.0/"><afp:link>%s</afp:link></rdf:Description></rdf:RDF>'
              '</x:xmpmeta><?xpacket end="w"?>' % html.escape(link, quote=True)).encode("utf-8")
    seg = b"\xff\xe1" + struct.pack(">H", 2 + len(XMP_HEAD) + len(packet)) + XMP_HEAD + packet
    out, i, put = bytearray(jpeg[:2]), 2, False
    while i + 4 <= len(jpeg) and jpeg[i] == 0xFF:
        m, n = jpeg[i + 1], struct.unpack(">H", jpeg[i + 2:i + 4])[0]
        body = jpeg[i:i + 2 + n]
        first = m == 0xE0 or (m == 0xE1 and body[4:10] == b"Exif\0\0")
        if not first and not put:
            out += seg
            put = True
        if m == 0xE1 and body[4:4 + len(XMP_HEAD)] == XMP_HEAD:
            i += 2 + n
            continue
        if m == 0xDA:
            break
        out += body
        i += 2 + n
    assert put, "no image data"
    return bytes(out) + jpeg[i:]


def small_jpeg(path):
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "s.jpg")
        subprocess.run(["/usr/bin/sips", "-s", "format", "jpeg", "-s", "formatOptions", "80", "-Z", "1280", path, "--out", out],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        with open(out, "rb") as f:
            return f.read()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    go, move = "--go" in sys.argv, "--move-renders" in sys.argv
    if len(args) != 1 or not os.path.isfile(os.path.join(args[0], "Project.afproject")):
        sys.exit(__doc__)
    root = os.path.abspath(args[0])
    todo = []
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x.lower() not in SKIP_DIRS and not x.startswith(".")]
        for name in sorted(files):
            low = name.lower()
            if low.endswith(".afplink.jpg") or not low.endswith((".heic", ".jpg", ".jpeg")):
                continue
            path = os.path.join(d, name)
            link = read_link(path)
            if not link or '"photo"' not in link:
                continue
            stem = name[:-7] if low.endswith(".mp.jpg") else name[:name.rfind(".")]
            todo.append((path, os.path.join(d, stem + ".afplink.jpg"), link, stem))
    made = moved = 0
    for path, obj, link, stem in todo:
        rel = os.path.relpath(path, root)
        if os.path.exists(obj):
            print("has its link already:", rel)
        else:
            print("link:  ", os.path.relpath(obj, root))
            if go:
                data = with_link(small_jpeg(path), link)
                with open(obj, "xb") as f:
                    f.write(data)
                made += 1
        if not move:
            continue
        also = [path] + [p for p in (os.path.join(os.path.dirname(path), stem + e) for e in (".mov", ".MOV")) if os.path.isfile(p)]
        seen = set()
        for p in also:
            ino = os.stat(p).st_ino  # .mov and .MOV are one file on a disk that ignores case
            if ino in seen:
                continue
            seen.add(ino)
            to = os.path.join(root, "_Renders", "Photos", os.path.relpath(p, root))
            if os.path.exists(to):
                print("  stays (a file of that name is in _Renders already):", os.path.relpath(p, root))
                continue
            print("  render:", os.path.relpath(p, root), "->", os.path.relpath(to, root))
            if go:
                os.makedirs(os.path.dirname(to), exist_ok=True)
                os.rename(p, to)
                moved += 1
    print("%d photos with a link. %s" % (len(todo), "Made %d links, moved %d files." % (made, moved) if go else "Nothing done: add --go."))


if __name__ == "__main__":
    main()
