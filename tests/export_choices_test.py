"""An export's settings are registry.json's (mpv/scripts/workspace/registry.json, docs/export-settings.md): the player's
exportsettings.lua, the Library's Share dialog (share.js) and its render recipe (renders.rs) read them there
(tests/registry_test.py checks each does). What's still apart, checked here:
  * gyroflow-export takes each size (--resolution) and encoding (--encode): its own argument parser
  * each setting's flag is the one exportsettings.lua's apply sets and gyroflow-export's parser reads
  * the first choice is each setting's default in the player's options (gyroflow.conf's export_size, photo_size, …)
A choice added to the registry that the export doesn't take would fail the render. From the repository's folder:
python3 tests/export_choices_test.py"""
import json
import re

from twins import Checks, read

c = Checks("export choices")

reg = json.loads(read("mpv/scripts/workspace/registry.json"))["export"]
lua = read("mpv/scripts/gyroflow/exportsettings.lua")
export = read("vs-gyroflow/src/bin/gyroflow-export.rs")
plugin = read("mpv/scripts/gyroflow/main.lua")
settings = {s["key"]: s for s in reg["settings"]}
c.expect("the registry's export settings", set(settings) == {"size", "encode", "quality"}, str(sorted(settings)))
c.expect("each kind's settings are the registry's", all(set(v) <= set(settings) for v in reg["kinds"].values()), str(reg["kinds"]))

# gyroflow-export takes them
for ch in settings["size"]["choices"]:
    v = ch["value"]
    ok = v == "original" or (v.isdigit() and int(v) >= 16)
    c.expect(f"gyroflow-export --resolution {v}", ok and re.search(r'"original" \| ""', export))
encodes = set(re.findall(r'"(\w+)" => (?:false|true)', re.search(r'"--encode" => [^\n]*', export).group(0)))
for ch in settings["encode"]["choices"]:
    c.expect(f"gyroflow-export --encode {ch['value']}", ch["value"] in encodes, str(encodes))
for s in reg["settings"]:
    if s.get("flag"):
        c.expect(f"{s['key']}: exportsettings.lua sets {s['flag']}", f'"{s["flag"]}"' in lua)
        c.expect(f"{s['key']}: gyroflow-export reads {s['flag']}", f'"{s["flag"]}" =>' in export)

# The player's options start at each setting's default (the first choice)
for kind, keys in reg["kinds"].items():
    prefix = "export" if kind == "video" else "photo"
    for k in keys:
        m = re.search(rf'\b{prefix}_{k} = "([^"]*)"', plugin)
        c.expect(f"gyroflow/main.lua's {prefix}_{k} starts at the first choice", m and m.group(1) == settings[k]["choices"][0]["value"],
                 m and m.group(1))
c.done()
