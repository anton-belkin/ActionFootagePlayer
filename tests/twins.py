"""Helpers for the guard tests that read the same knowledge out of Lua, Rust, JS and Python sources
(tests/*_twin_test.py, tests/script_messages_test.py, ...). Not a test itself: run_tests.sh runs only *_test.*"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_DIRS = {"node_modules", "target", ".git", "build", "dist"}


def path(rel):
    return os.path.join(ROOT, rel)


def read(rel):
    with open(path(rel), encoding="utf-8") as f:
        return f.read()


def files(top, exts):
    """Every file under `top` (relative to the repository) with one of `exts`, relative paths, sorted"""
    out = []
    for d, dirs, names in os.walk(path(top)):
        dirs[:] = sorted(x for x in dirs if x not in SKIP_DIRS)
        for n in sorted(names):
            if n.endswith(tuple(exts)):
                out.append(os.path.relpath(os.path.join(d, n), ROOT))
    return out


COMMENT = {".lua": "--", ".py": "#", ".sh": "#", ".rs": "//", ".js": "//", ".c": "//", ".h": "//"}


def code_lines(rel):
    """(line number, text) of a file's lines that aren't whole-line comments (nor in a Lua --[[ ]] block)"""
    mark = COMMENT.get(os.path.splitext(rel)[1], "#")
    out, in_block = [], False
    for i, line in enumerate(read(rel).splitlines(), 1):
        s = line.strip()
        if in_block:
            if "]]" in s:
                in_block = False
            continue
        if mark == "--" and s.startswith("--[["):
            in_block = "]]" not in s
            continue
        if s.startswith(mark) or (mark == "//" and s.startswith("*")):
            continue
        out.append((i, line))
    return out


def block(text, start, what):
    """The text from the first { or [ after `start` (a regex) to its matching close"""
    m = re.search(start, text)
    if not m:
        raise SystemExit(f"can't find {what} ({start})")
    i = m.end()
    while text[i] not in "{[":
        i += 1
    open_, close = text[i], "}" if text[i] == "{" else "]"
    depth = 0
    for j in range(i, len(text)):
        if text[j] == open_:
            depth += 1
        elif text[j] == close:
            depth -= 1
            if depth == 0:
                return text[i:j + 1]
    raise SystemExit(f"unclosed {what}")


class Checks:
    def __init__(self, name):
        self.name, self.n, self.failed, self.warnings = name, 0, [], []

    def expect(self, what, ok, detail=""):
        self.n += 1
        if not ok:
            self.failed.append(what + (f": {detail}" if detail else ""))

    def warn(self, what):
        self.warnings.append(what)

    def done(self):
        for w in self.warnings:
            print("warning: " + w)
        for f in self.failed:
            print("FAIL " + f)
        print(f"{self.n} checks, {len(self.failed)} failed")
        sys.exit(1 if self.failed else 0)
