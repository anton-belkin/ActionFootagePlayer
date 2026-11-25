"""The settings a render is made with are registry.json's `settings` (mpv/scripts/workspace/registry.json): transform.lua
and the Library's renders.rs take their keys, order, valid values, neutral values and what a project owns from it, and
renders.rs's defaults_of reads gyroflow.conf by its conf names (tests/registry_test.py checks each reads it;
tests/render_settings.json and tests/gyroflow_conf_cases.json check what each gives, case by case). What the registry
says that's still kept apart, checked here:
  * each key's gyroflow.conf option and default are the gyroflow plugin's own (gyroflow/main.lua's `o`)
  * each key gyroflow-export takes (exported, cli) is in its argument parser (the command line's), and is a key of the
    settings JSON it reads (settings_json, apply_settings) that a saved view's settings carry (gyroflow/settings.lua
    of_view; export.lua hands the settings, never the flag: tests/settings_json_test.py)
  * each key's field in the warp's request (gpu) is in gyroflow/bundle.lua's player_args or gpupath.lua's gpu_settings
  * a key not exported says why (player_only)
From the repository's folder: python3 tests/render_keys_test.py"""
import json
import re

from twins import Checks, block, files, read

c = Checks("render keys")
reg = json.loads(read("mpv/scripts/workspace/registry.json"))
# (main.lua: the options; bundle.lua: player_args; gpupath.lua: gpu_settings)
plugin = "\n".join(read(f"mpv/scripts/gyroflow/{f}.lua") for f in ("main", "bundle", "gpupath"))
# gyroflow-export, all its parts (src/bin/gyroflow-export/: options.rs parses the command line and --settings)
export_rs = "\n".join(read(f) for f in files("vs-gyroflow/src/bin/gyroflow-export", [".rs"]))

GROUPS = {"bundle", "uw", "input", "opts"}
KINDS = {"enum", "bool", "number", "ratio", "curve"}
keys = [s["key"] for s in reg["settings"]]
c.expect("each key once", len(keys) == len(set(keys)), str(keys))

# The plugin's options and their defaults: `local o = { k = v, ... }`
o = {}
for k, v in re.findall(r'(\w+) = ("[^"]*"|true|false|-?[\d.]+)', re.search(r"\nlocal o = \{([^\n]*)", plugin).group(1)):
    o[k] = json.loads(v) if v not in ("true", "false") else v == "true"
args = block(plugin, r"\nlocal function player_args\(source\)", "gyroflow/bundle.lua player_args") + re.search(r"\ngpu_settings = function\(source\)\n(.*?)\nend\n", plugin, re.S).group(1)
settings_lua = read("mpv/scripts/gyroflow/settings.lua")
json_keys = {k["key"]: k for k in reg["settings_json"]}
parsed = set(re.findall(r'"(--[a-z0-9-]+)"(?: \| "[^"]*")* =>', export_rs))

for s in reg["settings"]:
    k = s["key"]
    c.expect(f"{k}: a group", s.get("group") in GROUPS, str(s.get("group")))
    c.expect(f"{k}: a kind", s.get("kind") in KINDS, str(s.get("kind")))
    if s["kind"] in ("enum", "ratio"):
        c.expect(f"{k}: its values", s.get("values"))
    if s["kind"] == "enum" and "neutral" in s:
        c.expect(f"{k}: its neutral value is one of them", s["neutral"] in s["values"], str(s["neutral"]))
    if s.get("conf"):
        c.expect(f"{k}: gyroflow.conf's {s['conf']} is the plugin's option", s["conf"] in o, s["conf"])
        c.expect(f"{k}: its default is the plugin's ({s['conf']} = {o.get(s['conf'])!r})", o.get(s["conf"]) == s.get("conf_default"),
                 repr(s.get("conf_default")))
        for v in s.get("not_in_conf", []):
            c.expect(f"{k}: not_in_conf {v} is one of its values", v in s.get("values", []))
    if s.get("exported"):
        c.expect(f"{k}: exported, by its flags", s.get("cli"))
        for flag in s.get("cli", []):
            c.expect(f"{k}: gyroflow-export reads {flag}", flag in parsed)
        field = s["gpu"] if s.get("gpu") in json_keys else k
        c.expect(f"{k}: in the settings JSON as {field}, which gyroflow-export reads", json_keys.get(field, {}).get("export"))
        c.expect(f"{k}: a saved view's settings carry {field} (settings.lua of_view)", re.search(rf"\bs\.{field}\b", settings_lua))
    else:
        c.expect(f"{k}: not exported, says why (player_only)", s.get("player_only"))
    if s.get("gpu"):
        c.expect(f"{k}: the warp's request has {s['gpu']} (player_args)", re.search(rf"\b{s['gpu']} = ", args))
c.done()
