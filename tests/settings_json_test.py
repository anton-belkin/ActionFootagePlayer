#!/usr/bin/env python3
"""The player hands gyroflow-export one settings JSON (--settings: docs/reference/contracts.md, "The settings"), never
the flags it stands for: a flag passed by hand is a setting that reaches one path and not another ("works in the
player but not in the export"). Fails when a Lua file of the player (code, not comments) quotes a flag that
registry.json's settings_json says a key stands for, unless ALLOWED says why (another program's flag of the same
name). Also: each key the export reads has a flag it stood for or says why not, and gen_reference's list of flags
knows --settings. From the repository's folder: python3 tests/settings_json_test.py"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REGISTRY = os.path.join(ROOT, "mpv", "scripts", "workspace", "registry.json")

# file (from mpv/scripts) -> {flag: why it's there}
ALLOWED = {
    "gyroflow/main.lua": {
        "--underwater": "gyroflow-project's (PROJECT_WRITER), writing a Gyroflow project, not gyroflow-export's",
        "--level": "gyroflow-project's, as --underwater",
        "--wide": "gyroflow-project's, as --underwater",
        "--horizon-lock": "gyroflow-project's, as --underwater",
        "--rotation": "gyroflow-project's, as --underwater",
        "--aspect": "gyroflow-project's, as --underwater",
        "--projection": "gyroflow-project's, as --underwater",
    },
}

# Keys the export reads that no flag stood for before the settings
NO_FLAG = {"preset", "stitching_distance"}


def code_of(text):
    """The Lua without its comments (-- to the line's end, outside strings; --[[ ]] blocks)"""
    text = re.sub(r"--\[(=*)\[.*?\]\1\]", "", text, flags=re.S)
    out = []
    for line in text.split("\n"):
        q, i, keep = None, 0, line
        while i < len(line):
            c = line[i]
            if q:
                if c == "\\":
                    i += 1
                elif c == q:
                    q = None
            elif c in "'\"":
                q = c
            elif line.startswith("--", i):
                keep = line[:i]
                break
            i += 1
        out.append(keep)
    return "\n".join(out)


def main():
    with open(REGISTRY) as f:
        reg = json.load(f)
    carried = {}
    failures = []
    for k in reg["settings_json"]:
        if k.get("export"):
            for flag in k.get("cli", []):
                carried[flag] = k["key"]
            if not k.get("cli") and k["key"] not in NO_FLAG:
                failures.append(f"registry.json settings_json {k['key']}: the export reads it, no cli and not in NO_FLAG")
    assert len(carried) > 30, carried
    scripts = os.path.join(ROOT, "mpv", "scripts")
    seen = 0
    for d, _, files in os.walk(scripts):
        for name in files:
            if not name.endswith(".lua"):
                continue
            path = os.path.join(d, name)
            rel = os.path.relpath(path, scripts).replace(os.sep, "/")
            with open(path, encoding="utf-8") as f:
                code = code_of(f.read())
            seen += 1
            for m in re.finditer(r"[\"']\s*(--[a-z0-9][a-z0-9-]*)[\"'= ]", code):
                flag = m.group(1)
                if flag in carried and flag not in ALLOWED.get(rel, {}):
                    line = code[:m.start()].count("\n") + 1
                    failures.append(f"mpv/scripts/{rel}:{line}: {flag} is a setting ({carried[flag]}): put it in the settings JSON, not a flag")
    assert seen > 20
    # Each allowance still needed
    for rel, flags in ALLOWED.items():
        with open(os.path.join(scripts, rel), encoding="utf-8") as f:
            code = code_of(f.read())
        for flag in flags:
            if f'"{flag}"' not in code:
                failures.append(f"ALLOWED {rel} {flag}: not there any more, take it out")
    # export.lua passes them as one JSON
    with open(os.path.join(scripts, "gyroflow", "export.lua"), encoding="utf-8") as f:
        if '"--settings"' not in code_of(f.read()):
            failures.append("export.lua doesn't pass --settings")
    for x in failures:
        print("FAIL", x)
    print(f"{len(carried)} flags the settings carry, {seen} Lua files, {len(failures)} failures")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
