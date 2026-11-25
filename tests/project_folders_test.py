"""Every folder the player and the Library make in a project (_Moments, _Stills, _Renders, Generated, .afp, …) is left
out wherever a project's footage is listed (#128: _Stills was taken for footage, its Live Photos' .mov files as clips).

The folders made are found in the code (mpv/scripts, vs-gyroflow, catalog, library, tools/stream): every "_Name"
string, and "Generated", ".afp", "stabilized" where a path is made of them. A new "_Name" that isn't a folder (a
VapourSynth frame property, say) goes into NOT_FOLDERS.

Each place that lists footage says which it leaves out: a list of the project's own folders (lower case), a rule for
names starting with _ or ., or both (SCANNERS). A folder made has to be left out by each, or be in KNOWN with its issue.
The lists themselves agree (the same names, "#recycle" aside: the walks skip # names anyway).
From the repository's folder: python3 tests/project_folders_test.py"""
import re

from twins import Checks, block, code_lines, files, read

c = Checks("project folders")

# "_Name" strings that aren't folders a project has
NOT_FOLDERS = {
    "_AbsoluteTime", "_ColorRange", "_DurationDen", "_DurationNum", "_GyroflowSkipped",  # VapourSynth frame properties
}
# The places that list a project's footage: the file, how its own-folders list is written (None: it has none), and the
# rules it has for _ and . names, each as a regex that must be in its code (None: it doesn't have that rule)
SCANNERS = {
    "library/src/index.rs": {"list": r"pub const OWN_FOLDERS: \[&str; \d+\] = ", "under": r"top\.starts_with\('_'\)",
                             "dot": r"name\.starts_with\('\.'\)"},
    "catalog/src/inspect.rs": {"list": r"pub const OWN_FOLDERS: \[&str; \d+\] = ", "under": r"top\.starts_with\('_'\)",
                               "dot": r"name\.starts_with\('\.'\)"},
    # checks.rs walks with index.rs's walk (which skips . and # names)
    "library/src/checks.rs": {"list": r"const OWN: \[&str; \d+\] = ", "under": None, "dot": r"crate::index::walk\("},
    "mpv/scripts/workspace/moments.lua": {"list": r"local OWN_FOLDERS = ", "under": None, "dot": r'name:sub\(1, 1\) ~= "\."'},
    # The NAS stream's catalog: _ folders are left out (_Moments and _Clips listed as moments and clip previews)
    "tools/stream/afp_stream.py": {"list": None, "under": r'p\.startswith\("_"\)', "dot": r'x\.startswith\("\."\)'},
}
# Folders made that a scanner doesn't leave out yet: (scanner, folder) -> the issue
KNOWN = {
    ("tools/stream/afp_stream.py", "Generated"): "#302: Generated/ at a project's top (before 0.11) listed as footage by the NAS stream",
    ("tools/stream/afp_stream.py", "stabilized"): "#302: the same, gyroflow-export's stabilized/",
}

made = {}  # folder -> [where]
UNDER = re.compile(r'"(_[A-Z][A-Za-z]+)"')
PLAIN = re.compile(r'''(?:join(?:_path)?\([^()]*|(?:const \w+: &str|M\.\w+|local [A-Z_]+|GENERATED)\s*=\s*)"(Generated|stabilized|\.afp)"''')
sources = files("mpv/scripts", [".lua"]) + files("vs-gyroflow/src", [".rs"]) + files("catalog/src", [".rs"]) \
    + files("library/src", [".rs"]) + files("library/ui", [".js"]) + files("tools/stream", [".py"])
for f in sources:
    for n, line in code_lines(f):
        for m in UNDER.finditer(line):
            if m.group(1) not in NOT_FOLDERS:
                made.setdefault(m.group(1), []).append(f"{f}:{n}")
        for m in PLAIN.finditer(line):
            made.setdefault(m.group(1), []).append(f"{f}:{n}")
for name in ("_Moments", "_Stills", "_Renders", "_Exports", "_Clips", "Generated", ".afp"):
    c.expect(f"the folders made are found: {name}", name in made)
for name in NOT_FOLDERS:
    c.expect(f"NOT_FOLDERS lists only names in the code: {name}", any(name in read(f) for f in sources))

lists = {}
for f, s in SCANNERS.items():
    text = read(f)
    own = None
    if s["list"]:
        body = block(text, s["list"], f"{f}'s own folders")
        own = set(re.findall(r'"([^"]+)"', body)) | set(re.findall(r'\[?"?([#\w]+)"?\]?\s*=\s*true', body))
        own = {x for x in own if x != "true"}
        lists[f] = own
    for rule in ("under", "dot"):
        if s[rule]:
            c.expect(f"{f} has its rule for {rule} names", re.search(s[rule], text), s[rule])
    for name, where in sorted(made.items()):
        out = (own is not None and name.lower() in own) or (name.startswith("_") and s["under"]) or (name.startswith(".") and s["dot"])
        if (f, name) in KNOWN:
            c.expect(f"KNOWN still needed: {f} leaves out {name} now (take it out of KNOWN)", not out)
            continue
        c.expect(f"{f} leaves {name} out of the footage (made at {where[0]})", out)

# The lists agree
names = {f: sorted(x for x in own if x != "#recycle") for f, own in lists.items()}
first = next(iter(names))
for f, own in names.items():
    c.expect(f"{f}'s own folders are {first}'s", own == names[first], f"{f}: {own}; {first}: {names[first]}")
c.done()
