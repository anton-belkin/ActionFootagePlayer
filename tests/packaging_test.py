#!/usr/bin/env python3
# Every Lua module the player's plugins require is in the app and the install: starting from each plugin's main.lua
# (mpv/scripts/<plugin>/main.lua), every `require 'x'`, `require('x')` and `pcall(require, 'x')` is followed to its
# file (the plugin's own folder first, then another plugin's: some reach a neighbour's through package.path), and that
# file must be one tools/make_app.sh, tools/make_windows_app.sh and install.sh copy. mpv's own modules (mp, mp.*) aside;
# a plugin a packager doesn't ship at all (sdr-check: the macOS app's alone) is that packager's choice.
# A module missing from them leaves the built app's plugin failing to load (ahead.lua, PR #272).
# From the repository's folder: python3 tests/packaging_test.py

import os, re, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
SCRIPTS = os.path.join(ROOT, 'mpv', 'scripts')
PACKAGERS = ['tools/make_app.sh', 'tools/make_windows_app.sh', 'install.sh']
REQUIRE = re.compile(r"""(?:\brequire\s*\(?\s*|pcall\s*\(\s*require\s*,\s*)['"]([\w.]+)['"]""")

def module_file(plugin, name):
    own = os.path.join(SCRIPTS, plugin, name + '.lua')
    if os.path.isfile(own): return own
    for other in sorted(os.listdir(SCRIPTS)):
        f = os.path.join(SCRIPTS, other, name + '.lua')
        if os.path.isfile(f): return f
    return None

def needed(text):
    out, missing = set(), []
    for plugin in sorted(os.listdir(SCRIPTS)):
        main = os.path.join(SCRIPTS, plugin, 'main.lua')
        if not os.path.isfile(main) or f'mpv/scripts/{plugin}/main.lua' not in text: continue
        todo = [main]
        while todo:
            f = todo.pop()
            if f in out: continue
            out.add(f)
            code = re.sub(r'--[^\n]*', '', open(f, encoding='utf-8').read())   # not the ones in comments
            for name in REQUIRE.findall(code):
                if name == 'mp' or name.startswith('mp.'): continue
                m = module_file(plugin, name)
                if m: todo.append(m)
                else: missing.append(f'{os.path.relpath(f, ROOT)}: require {name!r}: no such file')
    return out, missing

failures, total = [], 0
for p in PACKAGERS:
    text = open(os.path.join(ROOT, p), encoding='utf-8').read()
    files, missing = needed(text)
    total = max(total, len(files))
    failures += [m for m in missing if m not in failures]
    for f in sorted(files):
        rel = os.path.relpath(f, ROOT).replace(os.sep, '/')
        if rel not in text: failures.append(f'{p} does not copy {rel}')
for line in failures: print('FAIL ' + line)
print(f'{total} modules, {len(PACKAGERS)} packagers, {len(failures)} failed')
sys.exit(1 if failures else 0)
