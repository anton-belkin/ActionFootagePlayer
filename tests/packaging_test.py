#!/usr/bin/env python3
# Every Lua module the player's plugins require is in the app and the install: starting from each plugin's main.lua
# (mpv/scripts/<plugin>/main.lua), every `require 'x'`, `require('x')` and `pcall(require, 'x')` is followed to its
# file (the plugin's own folder first, then another plugin's: some reach a neighbour's through package.path), and that
# file must be one tools/make_app.sh, tools/make_windows_app.sh and install.sh copy. mpv's own modules (mp, mp.*) and LuaJIT's aside;
# a plugin a packager doesn't ship at all (sdr-check: the macOS app's alone) is that packager's choice.
# A module missing from them leaves the built app's plugin failing to load (ahead.lua, PR #272).
# A module's data files (DATA: registry.lua's registry.json, the lists shared with gyroflow-export and the Library) go
# wherever the module goes. The plugins zipped alone (tools/make_release.sh, tools/make_plugin_zip.sh) carry the
# registry beside their own files when they need it: they'd fail to load without it.
# From the repository's folder: python3 tests/packaging_test.py

import os, re, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
SCRIPTS = os.path.join(ROOT, 'mpv', 'scripts')
PACKAGERS = ['tools/make_app.sh', 'tools/make_windows_app.sh', 'install.sh']
# LuaJIT's and Lua's own
BUILTIN = {'ffi', 'bit', 'jit', 'string', 'table', 'math', 'io', 'os', 'coroutine', 'debug', 'package', 'utf8'}
# A module -> the files it reads beside itself
DATA = {'mpv/scripts/workspace/registry.lua': ['mpv/scripts/workspace/registry.json']}
ZIPPERS = ['tools/make_release.sh', 'tools/make_plugin_zip.sh']
REQUIRE = re.compile(r"""(?:\brequire\s*\(?\s*|pcall\s*\(\s*require\s*,\s*)['"]([\w.]+)['"]""")

def module_file(plugin, name):
    own = os.path.join(SCRIPTS, plugin, name + '.lua')
    if os.path.isfile(own): return own
    for other in sorted(os.listdir(SCRIPTS)):
        f = os.path.join(SCRIPTS, other, name + '.lua')
        if os.path.isfile(f): return f
    return None

def needed(text, only=None):
    out, missing = set(), []
    for plugin in sorted(os.listdir(SCRIPTS)):
        main = os.path.join(SCRIPTS, plugin, 'main.lua')
        if not os.path.isfile(main) or (only is None and f'mpv/scripts/{plugin}/main.lua' not in text) or (only and plugin != only): continue
        todo = [main]
        while todo:
            f = todo.pop()
            if f in out: continue
            out.add(f)
            code = re.sub(r'--[^\n]*', '', open(f, encoding='utf-8').read())   # not the ones in comments
            for name in REQUIRE.findall(code):
                if name == 'mp' or name.startswith('mp.') or name in BUILTIN: continue
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
        for data in DATA.get(rel, []):
            if data not in text: failures.append(f'{p} copies {rel} but not {data}, which it reads')
for p in ZIPPERS:
    text = open(os.path.join(ROOT, p), encoding='utf-8').read()
    for line in re.findall(r'git archive HEAD (mpv/scripts/[^|]*)\|', text):
        plugin = line.split()[0].split('/')[-1]
        files, _ = needed('', only=plugin)
        rels = {os.path.relpath(f, ROOT).replace(os.sep, '/') for f in files}
        if 'mpv/scripts/workspace/registry.lua' in rels and plugin != 'workspace':
            put = re.search(rf'with_registry "[^"]*/{re.escape(plugin)}"', text) or \
                all(re.search(rf'git show HEAD:{re.escape(f)} > "[^"]*/{re.escape(plugin)}/{os.path.basename(f)}"', text)
                    for f in ['mpv/scripts/workspace/registry.lua'] + DATA['mpv/scripts/workspace/registry.lua'])
            if not put: failures.append(f'{p}: the {plugin} plugin zipped alone needs registry.lua and registry.json beside it')
for line in failures: print('FAIL ' + line)
print(f'{total} modules, {len(PACKAGERS)} packagers, {len(failures)} failed')
sys.exit(1 if failures else 0)
