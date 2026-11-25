"""The settings a render is made with are the same keys and values in the player and the Library, so a key added to one
can't silently skip the Library's recipe and its hash (tests/render_settings.json checks what each gives, case by case;
this checks that both have the same keys to give it from):
  * the bundle's keys: mpv/scripts/workspace/transform.lua KEYS and library/src/renders.rs BUNDLE_KEYS, in order
  * the underwater colour's: transform.lua UW_KEYS and renders.rs UW_KEYS
  * the input's (curve, rotation, underwater: the source's): each one renders.rs carries from an old view
  * each key's valid values: transform.lua VALID and renders.rs valid()
  * the values renders.rs defaults_of takes from gyroflow.conf are valid ones
A key only the player has goes into PLAYER_ONLY with why. From the repository's folder: python3 tests/render_keys_test.py"""
import re

from twins import Checks, block, read

c = Checks("render keys")
lua = read("mpv/scripts/workspace/transform.lua")
rs = read("library/src/renders.rs")

# Keys the player has that the Library's recipe doesn't need: why
PLAYER_ONLY = {}


def lua_list(name):
    m = re.search(rf"\nM\.{name} = \{{([^}}]*)\}}", lua)
    c.expect(f"transform.lua has {name}", m)
    return re.findall(r'"(\w+)"', m.group(1)) if m else []


def rs_list(name):
    m = re.search(rf"const {name}: \[&str; \d+\] = \[([^\]]*)\]", rs)
    c.expect(f"renders.rs has {name}", m)
    return re.findall(r'"(\w+)"', m.group(1)) if m else []


keys, bundle = lua_list("KEYS"), rs_list("BUNDLE_KEYS")
c.expect("the bundle's keys: transform.lua KEYS and renders.rs BUNDLE_KEYS, in order",
         [k for k in keys if k not in PLAYER_ONLY] == bundle, f"transform: {keys}; renders: {bundle}")
uw, uw_rs = lua_list("UW_KEYS"), rs_list("UW_KEYS")
c.expect("the underwater colour's keys: transform.lua UW_KEYS and renders.rs UW_KEYS",
         [k for k in uw if k not in PLAYER_ONLY] == uw_rs, f"transform: {uw}; renders: {uw_rs}")
for k in lua_list("INPUT_KEYS"):
    c.expect(f"renders.rs carries the input's {k} from an old view (transform.migrate_source)", f'v["{k}"]' in rs)
for k in PLAYER_ONLY:
    c.expect(f"PLAYER_ONLY lists only the player's keys: {k}", k in keys + uw)

# Valid values, key by key (aspect and uw_strength are worked out, not listed: both sides take any ratio, 0-1)
valid_lua = {}
for k, body in re.findall(r"\n    (\w+) = \{([^}]*)\},", block(lua, r"\nlocal VALID = ", "transform.lua VALID")):
    vals = set(re.findall(r'\["([^"]+)"\]|(\w+) = true', body))
    vals = {a or b for a, b in vals}
    if "[true] = true" in body:
        vals = {"<bool>"}
    valid_lua[k] = vals
valid_rs = {}
fn = block(rs, r"fn valid\(k: &str, v: &Value\) -> bool ", "renders.rs valid()")
for keys_, vals in re.findall(r'((?:"\w+"\s*\|?\s*)+) => matches!\(s, ([^)]*)\)', fn):
    for k in re.findall(r'"(\w+)"', keys_):
        valid_rs[k] = set(re.findall(r'"([^"]+)"', vals))
for keys_ in re.findall(r'((?:"\w+"\s*\|?\s*)+) => v\.is_boolean\(\)', fn):
    for k in re.findall(r'"(\w+)"', keys_):
        valid_rs[k] = {"<bool>"}
for k in sorted(set(bundle + uw_rs) - {"aspect", "uw_strength"}):
    c.expect(f"{k}: the same valid values in transform.lua and renders.rs", valid_lua.get(k) == valid_rs.get(k) and valid_rs.get(k),
             f"transform: {sorted(valid_lua.get(k, []))}; renders: {sorted(valid_rs.get(k, []))}")
for k in ("aspect", "uw_strength"):
    c.expect(f"{k}: worked out on both sides", k in fn and (k in valid_lua or f'k == "{k}"' in lua))

# defaults_of: what it takes from gyroflow.conf, a valid value
defaults = block(rs, r"fn defaults_of\(conf: &str\) -> Value ", "renders.rs defaults_of")
for key, of, default in re.findall(r'one\("(\w+)", &\[([^\]]*)\], "([^"]*)"\)', defaults):
    vals = set(re.findall(r'"([^"]+)"', of))
    if key in valid_rs:
        c.expect(f"defaults_of's {key}: valid values", vals <= valid_rs[key], f"{sorted(vals - valid_rs[key])}")
    c.expect(f"defaults_of's {key}: its default is one of them", default in vals)
c.done()
