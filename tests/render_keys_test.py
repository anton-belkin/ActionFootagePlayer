"""The settings a render is made with are registry.json's `settings` (mpv/scripts/workspace/registry.json): transform.lua
and the Library's renders.rs take their keys, order, valid values, neutral values and what a project owns from it, and
renders.rs's defaults_of reads gyroflow.conf by its conf names (tests/registry_test.py checks each reads it;
tests/render_settings.json and tests/gyroflow_conf_cases.json check what each gives, case by case). What the registry
says that's still kept apart, checked here:
  * each key's gyroflow.conf option and default are the gyroflow plugin's own (gyroflow/main.lua's `o`)
  * each key gyroflow-export takes (exported, cli) is in its argument parser and handed to it by export.lua
  * each key's field in the warp's request (gpu) is in gyroflow/main.lua's player_args
  * a key not exported says why (player_only)
From the repository's folder: python3 tests/render_keys_test.py"""
import json
import re

from twins import Checks, block, read

c = Checks("render keys")
reg = json.loads(read("mpv/scripts/workspace/registry.json"))
plugin = read("mpv/scripts/gyroflow/main.lua")
export_lua = read("mpv/scripts/gyroflow/export.lua")
export_rs = read("vs-gyroflow/src/bin/gyroflow-export.rs")

GROUPS = {"bundle", "uw", "input", "opts"}
KINDS = {"enum", "bool", "number", "ratio", "curve"}
keys = [s["key"] for s in reg["settings"]]
c.expect("each key once", len(keys) == len(set(keys)), str(keys))

# The plugin's options and their defaults: `local o = { k = v, ... }`
o = {}
for k, v in re.findall(r'(\w+) = ("[^"]*"|true|false|-?[\d.]+)', re.search(r"\nlocal o = \{([^\n]*)", plugin).group(1)):
    o[k] = json.loads(v) if v not in ("true", "false") else v == "true"
args = block(plugin, r"\nlocal function player_args\(source\)", "gyroflow/main.lua player_args")
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
            c.expect(f"{k}: export.lua hands it {flag}", f'"{flag}"' in export_lua)
    else:
        c.expect(f"{k}: not exported, says why (player_only)", s.get("player_only"))
    if s.get("gpu"):
        c.expect(f"{k}: the warp's request has {s['gpu']} (player_args)", re.search(rf"\b{s['gpu']} = ", args))
c.done()
