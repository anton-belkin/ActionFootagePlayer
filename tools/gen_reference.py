#!/usr/bin/env python3
"""The generated lists in docs/reference/contracts.md, read from the sources:

  script-messages  every script message a plugin registers (mp.register_script_message), the module that registers
                   it, and the files that send it by name
  user-data        every user-data/<key> written (set_property*, the plugins' publish loops) and read
  env              every AFP_* environment variable, where the product reads or sets it, and where rigs/tests do
  export-flags     every option gyroflow-export takes (its argument match), the first line of its --help text, and
                   who passes it by name

Each list sits between <!-- BEGIN GENERATED: <name> --> and <!-- END GENERATED: <name> --> in contracts.md.
  python3 tools/gen_reference.py           rewrites them
  python3 tools/gen_reference.py --check   exits 1 and prints what differs (tests/reference_test.py runs this)
No line numbers on purpose: the lists change when a contract does, not when code moves."""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tests"))
from twins import code_lines, dynamic_keys, files, read  # noqa: E402

DOC = "docs/reference/contracts.md"


def cell(paths, limit=6):
    paths = sorted(set(paths))
    shown = ", ".join(f"`{p}`" for p in paths[:limit])
    return shown + (f" and {len(paths) - limit} more" if len(paths) > limit else "") if paths else "—"


# ---------------------------------------------------------------- script messages
def script_name(folder):
    return re.sub(r"[^A-Za-z0-9]", "_", folder)


REQUIRE = re.compile(r"""require\s*\(?\s*['"]([\w.]+)['"]|pcall\(\s*require\s*,\s*['"]([\w.]+)['"]""")
REGISTER = re.compile(r'register_script_message\(\s*("[^"]+"|[^,]+),')
# Names made at run time (as tests/script_messages_test.py's DYNAMIC_MESSAGES)
DYNAMIC_MESSAGES = {'"part-" .. e': ["part-a", "part-b"]}


def script_messages():
    plugins = {script_name(d): d for d in sorted(os.listdir(os.path.join(ROOT, "mpv/scripts")))
               if os.path.isfile(os.path.join(ROOT, "mpv/scripts", d, "main.lua"))}
    modules = {}
    for f in files("mpv/scripts", [".lua"]):
        modules.setdefault(os.path.splitext(os.path.basename(f))[0], []).append(f)

    def closure(folder):
        seen, todo = [], [f"mpv/scripts/{folder}/main.lua"]
        while todo:
            f = todo.pop()
            if f in seen:
                continue
            seen.append(f)
            for _, line in code_lines(f):
                for a, b in REQUIRE.findall(line):
                    cands = modules.get(a or b, [])
                    own = [p for p in cands if p.startswith(f"mpv/scripts/{folder}/")]
                    if own or cands:
                        todo.append((own or cands)[0])
        return seen

    reg = {}  # (plugin, message) -> {files}
    for name, folder in plugins.items():
        for f in closure(folder):
            for _, line in code_lines(f):
                for m in REGISTER.finditer(line):
                    arg = m.group(1).strip()
                    names = DYNAMIC_MESSAGES.get(arg) or ([arg.strip('"')] if arg.startswith('"') else [])
                    for msg in names:
                        reg.setdefault((name, msg), set()).add(f)
    sends = {}
    send = "script-message" + "-to"  # (spelt apart: tests/script_messages_test.py reads tools/ for sends)
    to_list = re.compile(send + r'\\?"\s*,\s*\\?"([\w-]+)\\?"\s*,\s*\\?"([\w-]+)\\?"')
    to_text = re.compile(send + r' ([A-Za-z0-9_-]+) ([A-Za-z0-9_-]+)')
    sources = files("mpv/scripts", [".lua"]) + files("library/src", [".rs"]) + files("library/ui", [".js"]) \
        + files("tools", [".py", ".c", ".sh", ".lua", ".rs"]) + files("vs-gyroflow/src", [".rs"]) \
        + files("tests", [".py", ".lua"]) + files("macos", [".lua", ".sh"])
    for f in sources:
        for _, line in code_lines(f):
            for m in list(to_list.finditer(line)) + list(to_text.finditer(line)):
                sends.setdefault((script_name(m.group(1)), m.group(2)), set()).add(f)
    out = ["| Plugin | Message | Registered in | Sent by name from |", "|---|---|---|---|"]
    for (plugin, msg) in sorted(reg):
        out.append(f"| {plugin} | `{msg}` | {cell(reg[(plugin, msg)])} | {cell(sends.get((plugin, msg), []), 4)} |")
    out.append("")
    out.append(f"{len(reg)} messages in {len({p for p, _ in reg})} plugins. A message with no sender by name is sent "
               "by the panel's or menu's tables, a forwarding helper, or the Library's IPC with a computed name.")
    return out


# ---------------------------------------------------------------- user-data
def user_data():
    write_re = re.compile(r'(?:set_property(?:_native|_number|_bool|_string)?)\(\s*"user-data/([^"]+)"\s*(\.\.)?')
    any_re = re.compile(r'"user-data/([A-Za-z0-9_./-]*)\\?"\s*(\.\.)?')
    writes, reads = {}, {}
    sources = files("mpv/scripts", [".lua"]) + files("library/src", [".rs"]) + files("library/ui", [".js"]) \
        + files("tools/scrub", [".c"]) + files("tools/stream", [".py"])
    for f in sources:
        for n, line in code_lines(f):
            written = set()
            for m in write_re.finditer(line):
                key = m.group(1).rstrip("/")
                written.add(m.start(1))
                if m.group(2):
                    for k in dynamic_keys(f, n, line) or []:
                        writes.setdefault(f"{key}/{k}", set()).add(f)
                else:
                    writes.setdefault(key, set()).add(f)
            for m in any_re.finditer(line):
                if m.start(1) in written:
                    continue
                key = m.group(1).rstrip("/")
                if not key or (f.endswith(".c") and "observe_property" not in line):
                    continue
                reads.setdefault(key, set()).add(f)
    out = ["| Key (`user-data/…`) | Written by | Read by |", "|---|---|---|"]
    for key in sorted(set(writes) | set(reads)):
        out.append(f"| `{key}` | {cell(writes.get(key, []), 3)} | {cell(reads.get(key, []), 4)} |")
    out.append("")
    out.append("A key read with nothing listed as its writer is covered by a key above or below it (a table written "
               "whole, or its fields one by one); `tests/user_data_test.py` checks that every read has a writer.")
    return out


# ---------------------------------------------------------------- AFP_* environment
NOT_ENV = {"AFP_NS": "an XML namespace constant (albums.rs)", "AFP_RPU": "the afp-rpu path in dlogm/main.lua"}
PRODUCT = ("mpv/", "vs-gyroflow/src/", "catalog/src/", "library/src/", "macos/", "windows/", "tools/nas-helper/",
           "tools/stream/", "install.sh", ".github/")


def env():
    found = {}
    for f in files(".", [".lua", ".rs", ".py", ".js", ".sh", ".c", ".yml", ".yaml", ".swift", ".m", ".iss"]):
        if f.startswith(("docs/", "website/", "research_notes/")):
            continue
        for _, line in enumerate(read(f).splitlines()):
            for m in re.finditer(r"\bAFP_[A-Z0-9_]*[A-Z0-9]\b", line):
                if m.group(0) not in NOT_ENV:
                    found.setdefault(m.group(0), set()).add(f)
    out = ["| Variable | In the product | In tests, rigs and tools |", "|---|---|---|"]
    for var in sorted(found):
        prod = [f for f in found[var] if f.startswith(PRODUCT) and "/tests/" not in f and "/examples/" not in f]
        rest = [f for f in found[var] if f not in prod]
        out.append(f"| `{var}` | {cell(prod, 4)} | {cell(rest, 4)} |")
    out.append("")
    out.append(f"{len(found)} variables, found by name in code and comments (not docs/). Not variables: "
               + "; ".join(f"`{k}`, {v}" for k, v in sorted(NOT_ENV.items())) + ".")
    return out


# ---------------------------------------------------------------- gyroflow-export flags
EXPORT = "vs-gyroflow/src/bin/gyroflow-export.rs"


def export_flags():
    text = read(EXPORT)
    usage = re.search(r'const USAGE: &str = "(.*?)";\n', text, re.S).group(1).replace('\\"', '"')
    help_line, cur = {}, None
    for line in usage.splitlines():
        m = re.match(r"(?:\s{1,4}|[A-Z]\w+: )(--[a-z0-9-]+)(?:\s+(<[^>]+>))?\s+(.*)", line) \
            or re.match(r"\s+gyroflow-export (--[a-z0-9-]+) (<[^>]+>)\S*\s+\((.*?)\)?$", line)
        if m:
            cur = m.group(1) if m.group(1) not in help_line else None
            if cur:
                help_line[cur] = [m.group(2) or "", m.group(3).strip()]
        elif cur and re.match(r"\s{10,}\S", line):
            help_line[cur][1] += " " + line.strip()
        else:
            cur = None
    for flag, (takes, what) in help_line.items():
        # Its first sentence, at most ~160 characters
        what = re.split(r"(?<=[a-z0-9)])(?<!default)[.:] ", what.replace("e.g. ", "e.g.\0"))[0].replace("\0", " ")
        if len(what) > 160:
            what = what[:160].rsplit(" ", 1)[0] + " …"
        help_line[flag] = (takes, what)
    flags = []
    for m in re.finditer(r'^\s*((?:"--?[a-z0-9-]+"\s*\|\s*)*"--[a-z0-9-]+")\s*=>', text, re.M):
        for flag in re.findall(r'"(--[a-z0-9-]+)"', m.group(1)):
            if flag not in flags and flag != "--help":
                flags.append(flag)
    if '"--fingerprint"' in text:
        flags.insert(0, "--fingerprint")
    callers = {}
    for f in files("mpv/scripts", [".lua"]) + files("library/src", [".rs"]) + files("library/ui", [".js"]) \
            + files("tools/stream", [".py"]) + files("tools/nas-helper", [".py"]) + files("macos", [".sh", ".py", ".lua"]) \
            + files("windows", [".py", ".rs"]):
        t = read(f)
        if "gyroflow-export" not in t:
            continue  # another program's options of the same name
        for flag in flags:
            if re.search(r'["\'\s]' + re.escape(flag) + r'(?![a-z0-9-])', t):
                callers.setdefault(flag, set()).add(f)
    out = ["| Flag | Takes | What (from `--help`) | Passed by name from |", "|---|---|---|---|"]
    for flag in flags:
        takes, what = help_line.get(flag, ("", "*(not in --help)*"))
        what = what.replace("|", "\\|")
        out.append(f"| `{flag}` | {takes.replace('|', chr(92) + '|')} | {what} | {cell(callers.get(flag, []), 4)} |")
    out.append("")
    out.append(f"{len(flags)} flags, from the argument match in `{EXPORT}`; the full text is `gyroflow-export --help`.")
    return out


SECTIONS = {"script-messages": script_messages, "user-data": user_data, "env": env, "export-flags": export_flags}


def render(doc):
    for name, fn in SECTIONS.items():
        begin, end = f"<!-- BEGIN GENERATED: {name} -->", f"<!-- END GENERATED: {name} -->"
        if begin not in doc or end not in doc:
            raise SystemExit(f"{DOC}: no {begin} … {end}")
        head, rest = doc.split(begin, 1)
        _, tail = rest.split(end, 1)
        body = "\n".join(["", "<!-- Made by tools/gen_reference.py: don't edit by hand -->", ""] + fn() + [""])
        doc = head + begin + body + end + tail
    return doc


def main():
    path = os.path.join(ROOT, DOC)
    with open(path, encoding="utf-8") as f:
        old = f.read()
    new = render(old)
    if "--check" in sys.argv[1:]:
        if new != old:
            import difflib
            sys.stdout.writelines(list(difflib.unified_diff(old.splitlines(True), new.splitlines(True), DOC, DOC + " (generated)"))[:80])
            print(f"\n{DOC}'s generated lists are out of date: run python3 tools/gen_reference.py and commit the result")
            return 1
        return 0
    if new != old:
        with open(path, "w", encoding="utf-8") as f:
            f.write(new)
        print(f"{DOC} updated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
