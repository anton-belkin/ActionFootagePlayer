"""Every script message sent reaches a plugin that takes it, and no key binding takes one over (#225: a binding named
`lens` took the gyroflow plugin's `lens` message, so the Lens drop-down did nothing).

Read from the sources, no mpv:
  * a plugin is a folder of mpv/scripts with a main.lua; mpv names it by the folder, other characters than letters and
    digits as _ (side-panel: side_panel). Its code is main.lua and every module it requires, wherever they are
    (workspace/*.lua and side-panel/*.lua are required by several plugins)
  * sent: `script-message-to <plugin> <message>` in a command string, `"script-message-to", "<plugin>", "<message>"`
    as a command's list (Lua commandv, the Library's IPC json!, a rig's ipc.send), in mpv/scripts, library/ and tools/
  * `script-binding <plugin>/<name>` (the panel's and menu's shortcuts) names a binding of that plugin
  * taken: mp.register_script_message("<message>", ...) in the plugin's code
  * mpv registers every key binding's name as a script message too: a binding of the same name as a message in the
    same plugin takes it over, whichever came first. Bindings named at run time ("rate-" .. n) count by their prefix
A send whose plugin or message is worked out at run time is listed in DYNAMIC below, with why; a new one fails until
it is listed. From the repository's folder: python3 tests/script_messages_test.py"""
import os
import re

from twins import Checks, code_lines, files, path, read

c = Checks("script messages")

# Sends this test can't resolve, by the text that sends them (a regex on the line): why each is fine
DYNAMIC = {
    # The answer to a request goes back to whoever asked (the request's `reply`, its `done`): the asker registered it
    r'"script-message-to", req\.reply': "a reply to the plugin that asked",
    # tagchips.lua's clicks: tag-pick, tag-toggle, tag-rate, tag-ask, each checked by tests/sidepanel_test.lua
    r'mp\.commandv\("script-message-to", "workspace", \.\.\.\)': "the tag chips' commands (tagchips.lua)",
    # afp-scrub's reply to the side panel's scrub.lua: `script-message-to <script> scrub <reply>`, the script named
    r'\\"script-message-to\\",%s,\\"scrub\\"': "afp-scrub answers the script that asked (scrub.lua registers scrub)",
    # The notification dialog's answer, to the plugin that asked (notify_client.lua registers notify-answer)
    r'"script-message-to", d\.reply, "notify-answer"': "a dialog's answer to the plugin that asked",
    # mpv's context menu, opened or picked from as the mouse button goes down or up
    r'"script-message-to", "context_menu", info_\.event': "mpv's own context_menu script",
}
# Plugins that aren't ours: mpv's own scripts
EXTERNAL = {"context_menu": "mpv's context menu script (mpv 0.40+), opened by the footage menu",
            "osc": "mpv's on-screen controller (osc.lua), hidden while the agent's report is drawn"}
# Messages registered under a name made at run time: the names (the expression, as written)
DYNAMIC_MESSAGES = {
    '"part-" .. e': ["part-a", "part-b"],      # side-panel: the A-B part's ends, for e in { "a", "b" }
}
# Bindings named at run time: the prefix each name starts with (the expression that makes it, as written)
DYNAMIC_BINDINGS = {
    '"key-" .. n': "key-",                      # any-layout: one per Cyrillic key and modifier
    '"rate-" .. n': "rate-",                    # workspace: 0-5 stars
    '"report-drag-" .. i': "report-drag-",      # agent: the report's mouse buttons
    '"notify-dlg-" .. k[1]': "notify-dlg-",     # side-panel/notify.lua: the dialog's keys
    'b[2]': None,                               # gestures.lua: its own table's names, <prefix>-key-<what> (below)
}


def script_name(folder):
    return re.sub(r"[^A-Za-z0-9]", "_", folder)


def call_args(text, start):
    """The arguments of the call whose ( is at `start`, as written, split at the top level's commas"""
    args, depth, cur, i, quote = [], 0, "", start, None
    while i < len(text):
        ch = text[i]
        if quote:
            cur += ch
            if ch == "\\":
                cur += text[i + 1]
                i += 1
            elif ch == quote:
                quote = None
        elif ch in "\"'":
            quote = ch
            cur += ch
        elif ch in "([{":
            depth += 1
            if depth > 1:
                cur += ch
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                args.append(cur.strip())
                return args
            cur += ch
        elif ch == "," and depth == 1:
            args.append(cur.strip())
            cur = ""
        else:
            cur += ch
        i += 1
    return args


LITERAL = re.compile(r'^"([^"\\]*)"$')

# The plugins, their modules (by require), each module's registrations and bindings
plugins = {}
for d in sorted(os.listdir(path("mpv/scripts"))):
    if os.path.isfile(path(f"mpv/scripts/{d}/main.lua")):
        plugins[script_name(d)] = d
modules = {}  # name -> [paths], every folder's module of that name
for f in files("mpv/scripts", [".lua"]):
    modules.setdefault(os.path.splitext(os.path.basename(f))[0], []).append(f)

REQUIRE = re.compile(r"""require\s*\(?\s*['"]([\w.]+)['"]|pcall\(\s*require\s*,\s*['"]([\w.]+)['"]""")


def closure(folder):
    """The plugin's files: main.lua and what it requires, its own folder's module first"""
    seen, todo = [], [f"mpv/scripts/{folder}/main.lua"]
    while todo:
        f = todo.pop()
        if f in seen:
            continue
        seen.append(f)
        for _, line in code_lines(f):
            for a, b in REQUIRE.findall(line):
                name = a or b
                if name in ("mp", "mp.utils", "mp.options", "mp.msg", "mp.input"):
                    continue
                cands = modules.get(name, [])
                own = [p for p in cands if p.startswith(f"mpv/scripts/{folder}/")]
                if own or cands:
                    todo.append((own or cands)[0])
    return seen


def registered(f):
    out = []
    for n, line in code_lines(f):
        for m in re.finditer(r"register_script_message\(", line):
            args = call_args(line, m.end() - 1)
            lit = LITERAL.match(args[0]) if args else None
            if args and args[0] in DYNAMIC_MESSAGES:
                out += [(x, f, n) for x in DYNAMIC_MESSAGES[args[0]]]
                continue
            c.expect(f"{f}:{n}: a script message's name is written out, or listed in DYNAMIC_MESSAGES", lit, line.strip())
            if lit:
                out.append((lit.group(1), f, n))
    return out


def bindings(f):
    """(name or None, prefix or None, file, line) of each key binding made"""
    out = []
    text_lines = code_lines(f)
    for n, line in text_lines:
        for m in re.finditer(r"add_(?:forced_)?key_binding\(", line):
            args = call_args(line, m.end() - 1)
            if len(args) < 2 or args[1] == "nil":
                continue
            lit = LITERAL.match(args[1])
            if lit:
                out.append((lit.group(1), None, f, n if not line[:1].isspace() else -n))
            elif args[1] in DYNAMIC_BINDINGS:
                out.append((None, DYNAMIC_BINDINGS[args[1]], f, n))
            else:
                c.expect(f"{f}:{n}: a binding named at run time is listed in DYNAMIC_BINDINGS", False, args[1])
        # A table of bindings, { key, name, fn } (gestures.lua's, crop.lua's): named <prefix>-key-<what>
        for m in re.finditer(r'\{\s*"[^"]+",\s*"([A-Za-z0-9][\w-]*)",\s*(?:function|button|wheel)\b', line):
            out.append((m.group(1), None, f, n))
        if re.search(r'\{\s*"[^"]+",\s*name\("', line):
            out.append((None, "-key-", f, n))
    return out


top_level = {}  # file -> lines of top-level register_script_message calls
for f in files("mpv/scripts", [".lua"]):
    top_level[f] = {n for n, line in code_lines(f) if line.startswith("mp.register_script_message(")}
reg = {}      # plugin -> {message: [(file, line)]}
binds = {}    # plugin -> [(name, prefix, file, line)]
for name, folder in plugins.items():
    reg[name], binds[name] = {}, []
    for f in closure(folder):
        for msg, ff, n in registered(f):
            reg[name].setdefault(msg, []).append((ff, n))
        binds[name] += bindings(f)
    for msg, where in reg[name].items():
        places = sorted({w[0] for w in where})
        c.expect(f"{name}: {msg} registered by one module of the plugin (a second one takes it over)", len(places) == 1, ", ".join(places))
    for bname, prefix, f, n in binds[name]:
        if bname is not None:
            # Named as a message: fine only when the message is registered after the binding, both when the file is
            # loaded (top level, same file): the message's handler is then the one mpv keeps. Anything else: whichever
            # runs last wins, and #225 was the binding winning
            where = reg[name].get(bname, [])
            after = n > 0 and where and all(wf == f and wn > n and wn in top_level.get(wf, ()) for wf, wn in where)
            c.expect(f"{name}: the binding {bname} ({f}:{abs(n)}) isn't named as a script message of the plugin, or the message is "
                     "registered after it, both at the top level of one file (else the binding takes it over, #225)",
                     not where or after, f"registered at {where}")
        elif prefix == "-key-":
            for msg in reg[name]:
                c.expect(f"{name}: no script message named like a gesture binding (<prefix>-key-…)", "-key-" not in msg, msg)
        elif prefix:
            for msg in reg[name]:
                c.expect(f"{name}: no script message named like the bindings {prefix}… ({f}:{n})", not msg.startswith(prefix), msg)

# The sends, everywhere code is
TO_LIST = re.compile(r'script-message-to"\s*,\s*"([^"]+)"\s*,\s*"([^"]+)"')
TO_TEXT = re.compile(r'script-message-to ([A-Za-z0-9_-]+) ([A-Za-z0-9_-]+)')
TO_SELF = re.compile(r'script-message-to"\s*,\s*mp\.get_script_name\(\)\s*,\s*"([^"]+)"')
# A helper that sends whatever it's given to one plugin: `local function cmd(...) mp.commandv("script-message-to", "gyroflow", ...) end`,
# `local function msg(...) return "script-message-to view360 " .. table.concat({ ... }, " ") end`; its calls in that file are sends
FORWARDER = re.compile(r'local function (\w+)\(\.\.\.\)\s*(?:return\s+)?(?:mp\.commandv\("script-message-to", "([\w-]+)", \.\.\.\)'
                       r'|"script-message-to ([\w-]+) " \.\. table\.concat\(\{ \.\.\. \}, " "\))')
owners = {}  # file -> the plugins whose code it is
for name, folder in plugins.items():
    for f in closure(folder):
        owners.setdefault(f, []).append(name)
sources = files("mpv/scripts", [".lua"]) + files("library/src", [".rs"]) + files("library/ui", [".js"]) \
    + files("tools", [".py", ".c", ".sh", ".lua", ".rs"]) + files("vs-gyroflow/src", [".rs"])
sends = 0
used_dynamic = set()
def check_send(f, n, line, plugin, msg):
    if plugin in EXTERNAL:
        return
    c.expect(f"{f}:{n}: script-message-to {plugin}: a plugin of ours", plugin in plugins, line.strip())
    if plugin in plugins:
        c.expect(f"{f}:{n}: {plugin} takes the message {msg}", msg in reg[plugin], line.strip())


for f in sources:
    lines = code_lines(f)
    forwarders = {}
    for n, line in lines:
        fw = FORWARDER.search(line)
        if fw:
            forwarders[fw.group(1)] = fw.group(2) or fw.group(3)
    for n, line in lines:
        if FORWARDER.search(line):
            continue
        for helper, plugin in forwarders.items():
            for m in re.finditer(r"\b" + helper + r'\("([\w-]+)"', line):
                sends += 1
                check_send(f, n, line, plugin, m.group(1))
        for m in re.finditer(r"script-message-to", line):
            rest = line[m.start():]
            own = TO_SELF.match(rest)
            if own:
                sends += 1
                for plugin in owners.get(f, []):
                    check_send(f, n, line, plugin, own.group(1))
                c.expect(f"{f}:{n}: a send to its own plugin is in a plugin's code", owners.get(f))
                continue
            hit = TO_LIST.match(rest) or TO_TEXT.match(rest)
            if not hit:
                why = [k for k in DYNAMIC if re.search(k, line)]
                c.expect(f"{f}:{n}: a send this test can't read is listed in DYNAMIC", why, line.strip())
                used_dynamic.update(why)
                continue
            sends += 1
            check_send(f, n, line, hit.group(1), hit.group(2))
        for m in re.finditer(r"script-binding ([A-Za-z0-9_-]+)/([A-Za-z0-9_-]+)", line):
            plugin, b = m.group(1), m.group(2)
            c.expect(f"{f}:{n}: script-binding {plugin}/…: a plugin of ours", plugin in plugins)
            if plugin in plugins:
                names = {x[0] for x in binds[plugin] if x[0]}
                prefixes = [x[1] for x in binds[plugin] if x[1] and x[1] != "-key-"]
                c.expect(f"{f}:{n}: {plugin} has the binding {b}", b in names or any(b.startswith(p) for p in prefixes))
c.expect("the sends were found", sends > 100, str(sends))
for k in DYNAMIC:
    c.expect("DYNAMIC lists only sends there are", k in used_dynamic, k)
c.done()
