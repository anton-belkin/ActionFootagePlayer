"""Every user-data/<key> a plugin (or the Library, or a helper) reads is written by someone: a name changed on one side
only leaves the reader with nil, silently (the panel showing a default, an export losing a setting).

Read from the sources, no mpv: mpv/scripts, library/, tools/ (the rigs in tests/ aren't counted):
  * written: set_property*("user-data/<key>", ...), and the plugins' publish loops,
    `for k, v in pairs(state) do mp.set_property_native("user-data/<plugin>/" .. k, v) end` (the keys of that `state`
    table) and `local function publish(k, v) mp.set_property_native("user-data/<plugin>/" .. k, v) end` (the keys its
    calls name)
  * read: any other "user-data/<key>" in code (get_property*, observe_property, the Library's IPC get_property, lists
    of properties to watch)
A read is covered by a write of the same key, of a key above it (a table written whole, a field of it read), or below
it (fields written one by one, the whole read). Keys written by mpv itself or by nobody of ours are in EXTERNAL.
Writes nobody reads are printed as warnings, not failures (the agent's snapshot reads them all, as the side panel may).
From the repository's folder: python3 tests/user_data_test.py"""
import re

from twins import Checks, code_lines, files, read

c = Checks("user-data")

# Keys ours read that someone else writes
EXTERNAL = {
    "osc/visibility": "mpv's osc.lua",
}

LITERAL_WRITE = re.compile(r'(?:set_property(?:_native|_number|_bool)?)\(\s*"user-data/([^"]+)"\s*(\.\.)?')
ANY = re.compile(r'"user-data/([A-Za-z0-9_./-]*)\\?"\s*(\.\.)?')


def table_keys(text):
    """The keys of a Lua table constructor's top level ({ a = 1, b = { c = 2 } }: a, b)"""
    keys, depth, i, quote = [], 0, 0, None
    while i < len(text):
        ch = text[i]
        if quote:
            if ch == "\\":
                i += 1
            elif ch == quote:
                quote = None
        elif text.startswith("--", i):
            i = text.find("\n", i)
            if i < 0:
                break
        elif ch in "\"'":
            quote = ch
        elif ch in "{[(":
            depth += 1
        elif ch in "}])":
            depth -= 1
            if depth == 0:
                return keys, i
        elif depth == 1:
            m = re.match(r"([A-Za-z_]\w*)\s*=(?!=)", text[i:])
            if m and not re.match(r"[\w.]", text[i - 1]):
                keys.append(m.group(1))
                i += m.end()
                continue
        i += 1
    return keys, i


def dynamic_keys(f, n, line):
    """The keys a dynamic write at line n of f publishes: its loop's table's, or its helper's calls'"""
    text = read(f)
    lines = text.splitlines()
    loop = re.search(r"for \w+(?:, \w+)? in pairs\((\w+)\) do mp\.set_property_native\(", line)
    if loop:
        table = loop.group(1)
        # The table's constructor nearest above
        for j in range(n - 1, 0, -1):
            if re.match(rf"\s*(?:local\s+)?{table}\s*=\s*{{", lines[j - 1]):
                start = sum(len(x) + 1 for x in lines[:j - 1])
                body = text[start:]
                keys, _ = table_keys(body[body.index("{"):])
                # and fields set on it afterwards, before the loop
                keys += re.findall(rf"\b{table}\.(\w+)\s*=(?!=)", "\n".join(lines[j:n - 1]))
                return keys
        return None
    helper = re.search(r"local function (\w+)\((\w+), \w+\) mp\.set_property_native\(\"user-data/[^\"]*\" \.\. \2", line)
    if helper:
        return re.findall(rf"\b{helper.group(1)}\(\"(\w+)\"", text)
    return None


writes, reads = {}, {}   # key -> [where]
dyn = []                 # (prefix, keys, where)
sources = files("mpv/scripts", [".lua"]) + files("library/src", [".rs"]) + files("library/ui", [".js"]) \
    + files("tools/scrub", [".c"]) + files("tools/stream", [".py"])
for f in sources:
    for n, line in code_lines(f):
        where = f"{f}:{n}"
        written = set()
        for m in LITERAL_WRITE.finditer(line):
            key = m.group(1).rstrip("/")
            written.add(m.start(1))
            if m.group(2):
                keys = dynamic_keys(f, n, line)
                c.expect(f"{where}: a user-data key made at run time can be read (a publish loop over a table, or a helper's calls)",
                         keys is not None, line.strip())
                dyn.append((key, keys or [], where))
            else:
                writes.setdefault(key, []).append(where)
        for m in ANY.finditer(line):
            if m.start(1) in written:
                continue
            key = m.group(1).rstrip("/")
            if m.group(2) and not key:
                continue  # "user-data/" .. plugin: any of them (the footage menu's state of a plugin)
            if f.endswith(".c") and "observe_property" not in line:
                continue
            reads.setdefault(key, []).append(where)

# What's written, as keys: the dynamic ones each
for prefix, keys, where in dyn:
    for k in keys:
        writes.setdefault(f"{prefix}/{k}", []).append(where)
c.expect("writes were found", len(writes) > 50, str(len(writes)))
c.expect("reads were found", len(reads) > 40, str(len(reads)))


def covered(key, by):
    return any(w == key or key.startswith(w + "/") or w.startswith(key + "/") for w in by)


for key, where in sorted(reads.items()):
    if key in EXTERNAL or not key:
        continue
    c.expect(f"user-data/{key} is written by someone", covered(key, writes), "read at " + ", ".join(where[:3]))
for key in EXTERNAL:
    c.expect(f"EXTERNAL lists only keys read: {key}", key in reads)
for key, where in sorted(writes.items()):
    if not covered(key, reads):
        c.warn(f"user-data/{key} is written ({where[0]}) but nothing of ours reads it by name")
c.done()
