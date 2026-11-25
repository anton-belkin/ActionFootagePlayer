"""An export's settings are the same choices everywhere they're offered or kept (docs/design/export-settings.md):
  * the player's: mpv/scripts/gyroflow/exportsettings.lua CHOICES (value, label, note), DEFAULT, KEYS
  * the Library's Share dialog: library/ui/share.js EXPORT_CHOICES (the same three), EXPORT_KEYS; its default is each
    setting's first choice
  * the Library's render recipe: library/src/renders.rs EXPORT_CHOICES (the values; a recipe keeps the ones that aren't
    the first), photos without "encode"
  * gyroflow-export takes each size (--resolution) and encoding (--encode); a photo's radius at its best is still.rs's
A choice added on one side only would be dropped as invalid by the other (a Share setting that never reaches the render,
a recipe hash that differs). From the repository's folder: python3 tests/export_choices_test.py"""
import re

from twins import Checks, block, read

c = Checks("export choices")

lua = read("mpv/scripts/gyroflow/exportsettings.lua")
js = read("library/ui/share.js")
rs = read("library/src/renders.rs")
export = read("vs-gyroflow/src/bin/gyroflow-export.rs")


def lua_choices(text):
    body = block(text, r"\nM\.CHOICES = ", "exportsettings.lua CHOICES")
    out = {}
    for key, inner in re.findall(r"(\w+) = (\{\s*\{.*?\}\s*\})\s*,?\s*(?=\w+ = \{|\}$)", body, re.S):
        out[key] = re.findall(r'\{\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)"\s*\}', inner)
    return out


def js_choices(text):
    body = block(text, r"const EXPORT_CHOICES = ", "share.js EXPORT_CHOICES")
    out = {}
    for key, inner in re.findall(r"(\w+): (\[\s*\[.*?\]\s*\])\s*,?\s*(?=\w+: \[|\}$)", body, re.S):
        out[key] = re.findall(r'\[\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)"\s*\]', inner)
    return out


player, share = lua_choices(lua), js_choices(js)
c.expect("the player's choices are read", set(player) == {"size", "encode", "quality"}, str(sorted(player)))
c.expect("the Share dialog's choices are read", set(share) == set(player), str(sorted(share)))
for key in player:
    c.expect(f"{key}: the player and the Share dialog offer the same choices, labels and notes", player[key] == share.get(key),
             f"\n  player: {player[key]}\n  share:  {share.get(key)}")

recipe = dict(re.findall(r'\("(\w+)", &\[([^\]]*)\]\)', block(rs, r"pub const EXPORT_CHOICES: \[\(&str, &\[&str\]\); \d+\] = ", "renders.rs EXPORT_CHOICES")))
recipe = {k: re.findall(r'"([^"]+)"', v) for k, v in recipe.items()}
for key in player:
    c.expect(f"{key}: the render recipe knows the same values, in order (the first is the default)",
             recipe.get(key) == [v for v, _, _ in player[key]], f"renders.rs: {recipe.get(key)}")
c.expect("the render recipe has no other settings", set(recipe) == set(player), str(sorted(recipe)))

# Defaults: the player's DEFAULT is each setting's first choice (the Library's default)
default = dict(re.findall(r'(\w+) = "([^"]*)"', re.search(r"\nM\.DEFAULT = (\{[^}]*\})", lua).group(1)))
for key in player:
    c.expect(f"{key}: the player's default is the first choice, as the Library's", default.get(key) == player[key][0][0], str(default.get(key)))

# Which settings each kind has
def kinds(text):
    return {k: re.findall(r'"(\w+)"', v) for k, v in re.findall(r'(\w+)\s*[=:]\s*[\[{]([^\]}]*)[\]}]', text)}


lua_keys = kinds(re.search(r"\nM\.KEYS = (\{.*\})", lua).group(1))
js_keys = kinds(re.search(r"const EXPORT_KEYS = (\{.*\});", js).group(1))
c.expect("a video's and a photo's settings: the player's are the Share dialog's", lua_keys == js_keys, f"{lua_keys} / {js_keys}")
photo_skips = set(re.findall(r'photo && key == "(\w+)"', rs))
c.expect("the render recipe leaves out of a photo what a photo hasn't", photo_skips == set(lua_keys.get("video", [])) - set(lua_keys.get("photo", [])),
         str(photo_skips))

# gyroflow-export takes them
for v, _, _ in player.get("size", []):
    ok = v == "original" or (v.isdigit() and int(v) >= 16)
    c.expect(f"gyroflow-export --resolution {v}", ok and re.search(r'"original" \| ""', export))
encodes = set(re.findall(r'"(\w+)" => (?:false|true)', re.search(r'"--encode" => [^\n]*', export).group(0)))
for v, _, _ in player.get("encode", []):
    c.expect(f"gyroflow-export --encode {v}", v in encodes, str(encodes))
still = read("vs-gyroflow/src/still.rs")
radius = re.search(r"pub const MAX_RADIUS: usize = (\d+);", still).group(1)
c.expect("the photo's radius at its best is still.rs's MAX_RADIUS", re.search(rf"\nM\.PHOTO_RADIUS_MAX = {radius}\b", lua), radius)
c.done()
