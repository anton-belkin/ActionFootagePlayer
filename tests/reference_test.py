"""The docs hold together:
  * docs/reference/contracts.md's generated lists match the sources (tools/gen_reference.py --check): a script
    message, user-data key, AFP_* variable or gyroflow-export flag added or removed updates them
  * every relative link in a Markdown file of the repository reaches a file that exists, and its #anchor a heading
    there (GitHub's way of naming them)
  * every docs/<…>.md named anywhere in the repository (code comments too) exists
  * docs/README.md lists every doc, and each docs/reference/ and docs/dev/ file says what it was checked against
From the repository's folder: python3 tests/reference_test.py"""
import os
import re
import subprocess
import sys
import unicodedata

from twins import ROOT, Checks, path, read

c = Checks("reference")

# 1. The generated lists
r = subprocess.run([sys.executable, path("tools/gen_reference.py"), "--check"], capture_output=True, text=True)
c.expect("docs/reference/contracts.md's generated lists are current (python3 tools/gen_reference.py)", r.returncode == 0,
         (r.stdout + r.stderr)[-3000:])

tracked = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True).stdout.split("\n")
tracked = [f for f in tracked if f and os.path.exists(path(f))]
markdown = [f for f in tracked if f.endswith(".md")]


def slug(heading):
    """GitHub's anchor for a heading: lower case, punctuation dropped, spaces as hyphens"""
    h = re.sub(r"`|\*\*|__|\[([^\]]*)\]\([^)]*\)", lambda m: m.group(1) or "", heading.strip()).lower()
    return "".join(ch for ch in h if ch in " -_" or unicodedata.category(ch)[0] in "LN").replace(" ", "-")


anchors = {}


def anchors_of(f):
    if f not in anchors:
        seen, out, fence = {}, set(), False
        for line in read(f).splitlines():
            if line.startswith("```"):
                fence = not fence
            if fence:
                continue
            m = re.match(r"#{1,6}\s+(.*?)\s*#*\s*$", line)
            if m:
                s = slug(m.group(1))
                n = seen.get(s, 0)
                seen[s] = n + 1
                out.add(s if n == 0 else f"{s}-{n}")
            for a in re.findall(r'<a (?:name|id)="([^"]+)"', line):
                out.add(a)
        anchors[f] = out
    return anchors[f]


# 2. Relative links in Markdown
LINK = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]]*\]\(([^)\s]+)\)")
links = 0
for f in markdown:
    fence = False
    for n, line in enumerate(read(f).splitlines(), 1):
        if line.lstrip().startswith("```"):
            fence = not fence
        if fence:
            continue
        for m in LINK.finditer(re.sub(r"`[^`]*`", "", line)):
            target = m.group(1) or m.group(2)
            if re.match(r"[a-z]+:", target) or target.startswith("//"):
                continue
            links += 1
            file_part, _, anchor = target.partition("#")
            dest = os.path.normpath(os.path.join(os.path.dirname(f), file_part)) if file_part else f
            if file_part.startswith("/"):
                dest = file_part.lstrip("/")
            ok = os.path.exists(path(dest))
            c.expect(f"{f}:{n}: the link {target} reaches a file", ok)
            if ok and anchor and dest.endswith(".md"):
                c.expect(f"{f}:{n}: the link {target} reaches a heading", anchor in anchors_of(dest) or anchor.lower() in anchors_of(dest))
c.expect("links were found", links > 100, str(links))

# 3. docs/… named in any text file
# Named as another repository's file
ELSEWHERE = {"docs/spherical-video-v2-rfc.md": "Google's spatial-media repository (vs-gyroflow/src/mp4meta.rs)"}
DOC_NAME = re.compile(r"(?<![\w/.-])(docs/[\w./-]+?\.md)\b")
for f in tracked:
    if f.endswith((".png", ".jpg", ".ico", ".icns", ".cube", ".mp4", ".lock", ".json")) or "/img/" in f:
        continue
    try:
        text = read(f)
    except (UnicodeDecodeError, IsADirectoryError):
        continue
    for n, line in enumerate(text.splitlines(), 1):
        for m in DOC_NAME.finditer(line):
            if m.group(1) not in ELSEWHERE:
                c.expect(f"{f}:{n}: {m.group(1)} exists", os.path.exists(path(m.group(1))))

# 4. The index and the reference's headers
docs = sorted(os.path.relpath(os.path.join(d, n), path("docs")) for d, _, names in os.walk(path("docs"))
              for n in names if n.endswith(".md"))
index = read("docs/README.md")
for d in docs:
    if d != "README.md":
        c.expect(f"docs/README.md lists docs/{d}", f"]({d})" in index)
for d in docs:
    if d.startswith(("reference/", "dev/")):
        head = "\n".join(read(f"docs/{d}").splitlines()[:8])
        c.expect(f"docs/{d} says what it was checked against in its first lines", re.search(r"Checked against [0-9a-f]{7}", head))
    if d.startswith("design/"):
        head = "\n".join(read(f"docs/{d}").splitlines()[:6])
        c.expect(f"docs/{d} starts with its status", re.search(r"^\*\*Status:\*\*|^Status:", head, re.M))
c.done()
