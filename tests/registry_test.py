"""registry.json (mpv/scripts/workspace/registry.json) is the one list of the curves, an export's settings, the
transform's keys and the project's own folders, and every program reads it rather than keeping a copy:
  * READERS: each place that used to keep its own copy reads the file (the player through registry.lua, Rust through
    include_str! and afp_catalog::registry, the Library's page through window.AFP_LISTS, put there by main.rs)
  * no copy creeps back: a source line quoting several of the same list's names (log curves, own folders, the bundle's
    keys, an export's sizes) is one, unless ALLOWED says why it has to be
The file's packaging is tests/packaging_test.py's. From the repository's folder: python3 tests/registry_test.py"""
import json
import os
import re

from twins import Checks, code_lines, files, path, read

c = Checks("registry")
REG = "mpv/scripts/workspace/registry.json"
reg = json.loads(read(REG))
for k in ("curves", "export", "settings", "own_folders", "recycle_bins"):
    c.expect(f"registry.json has {k}", k in reg)
ids = [x["id"] for x in reg["curves"]]
c.expect("each curve once", len(ids) == len(set(ids)), str(ids))
c.expect("own folders lower case", all(x == x.lower() for x in reg["own_folders"]))

LUA = r"require[ (]'registry'\)?"
RUST = r"crate::lists::get\(\)"
JS = r"window\.AFP_LISTS\."
READERS = {
    "mpv/scripts/workspace/registry.lua": r'"registry\.json"',
    "mpv/scripts/workspace/transform.lua": LUA,
    "mpv/scripts/workspace/moments.lua": LUA,
    "mpv/scripts/gyroflow/exportsettings.lua": LUA,
    "mpv/scripts/dlogm/main.lua": LUA,
    "mpv/scripts/footage-menu/main.lua": LUA,
    "mpv/scripts/side-panel/main.lua": LUA,
    "vs-gyroflow/src/lists.rs": r'include_str!\("\.\./\.\./mpv/scripts/workspace/registry\.json"\)',
    "vs-gyroflow/src/bin/gyroflow-export/main.rs": r"vs_gyroflow::lists::",
    "library/src/lists.rs": r'include_str!\("\.\./\.\./mpv/scripts/workspace/registry\.json"\)',
    "library/src/main.rs": r"js_init_script\(lists::page_script\(\)\)",
    "library/src/renders.rs": RUST,
    "library/src/tags.rs": RUST,
    "library/src/index.rs": RUST,
    "library/src/checks.rs": RUST,
    "library/ui/share.js": JS,
    "library/ui/app.js": JS,
    "library/ui/albums.js": JS,
    "library/ui-tests/lists.js": r'"registry\.json"',
    "library/ui-tests/page.js": r'require\("\./lists"\)',
}
for f, pattern in READERS.items():
    c.expect(f"{f} reads registry.json", re.search(pattern, read(f)), pattern)
# The include_str! paths lead to the file
for f in ("vs-gyroflow/src/lists.rs", "library/src/lists.rs"):
    m = re.search(r'include_str!\("([^"]+)"\)', read(f))
    c.expect(f"{f}'s include_str! is the file", m and os.path.samefile(os.path.join(os.path.dirname(path(f)), m.group(1)), path(REG)))

# Copies: a line with at least `least` of a list's names, each in quotes
LISTS = {
    "log curves": ([x["id"] for x in reg["curves"] if x["log"]], 3),
    "own folders": (reg["own_folders"], 4),
    "the bundle's keys": ([s["key"] for s in reg["settings"] if s["group"] == "bundle"], 4),
    "an export's sizes": ([ch["value"] for s in reg["export"]["settings"] if s["key"] == "size" for ch in s["choices"]], 3),
}
# (file, list) -> why it keeps one
ALLOWED = {
    ("catalog/src/inspect.rs", "own folders"): "the catalog is built from its folder alone (catalog/Dockerfile): a const, "
                                               "checked against the registry by registry.rs's test",
    ("catalog/src/range.rs", "log curves"): "the same: a match giving each curve a name kept for good (Range::Log), "
                                            "checked by registry.rs's test",
    ("mpv/scripts/gyroflow/main.lua", "the bundle's keys"): "not a list: the warp's request and the plugin's options, by key "
                                                            "(tests/render_keys_test.py checks them against the registry)",
}
sources = files("mpv/scripts", [".lua"]) + files("vs-gyroflow/src", [".rs"]) + files("catalog/src", [".rs"]) \
    + files("library/src", [".rs"]) + files("library/ui", [".js"]) + files("tools/stream", [".py"])
sources = [f for f in sources if "/vendor/" not in f]
found = set()
for f in sources:
    for n, line in code_lines(f):
        quoted = set(re.findall(r'"([^"]*)"', line)) | set(re.findall(r"'([^']*)'", line))
        for name, (names, least) in LISTS.items():
            hits = [x for x in names if x in quoted]
            if len(hits) >= least:
                found.add((f, name))
                if (f, name) not in ALLOWED:
                    c.expect(f"{f}:{n} keeps a copy of {name} ({', '.join(hits)}): read registry.json instead", False)
for key, why in ALLOWED.items():
    c.expect(f"ALLOWED still needed: {key[0]} ({key[1]})", key in found, why)
c.done()
