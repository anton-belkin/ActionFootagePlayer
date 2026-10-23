#!/usr/bin/env python3
# tools/bundle_runtime.py <app> <prefix>: copies Homebrew's mpv, ffmpeg and what they load into ActionFootagePlayer.app,
# so the app runs without Homebrew. tools/make_app.sh runs it when BUNDLE_RUNTIME is set (to <prefix>,
# `brew --prefix`).
#
#   Contents/Helpers/            mpv, ffmpeg, ffprobe. macos/player.sh (Contents/MacOS/) runs this mpv when it's there
#   Contents/Frameworks/         every library they link from <prefix>, and libMoltenVK.dylib, with
#                                Python.framework: VapourSynth's scripts (gyroflow.vpy) run in it, with the
#                                vapoursynth module in its site-packages. Pruned to what the plugins and the setup use
#   Contents/Resources/vulkan/   MoltenVK's ICD manifest, for the Vulkan loader (VK_DRIVER_FILES, player.sh)
#   Contents/Resources/bin/      with VapourSynth 74 and later, the `vapoursynth` command VSScript runs to find its
#                                Python (player.sh puts it in PATH). A script: in Helpers/, codesign would
#                                take it for code
#   Contents/Resources/ThirdParty/ for each Homebrew formula all this comes from, its license files and formula, and
#                                SOURCES.txt: each one's version and source (the GPL's and LGPL's terms)
#
# Each binary refers to the libraries it links by their place relative to it (@loader_path/...). Everything is
# signed ad hoc, as the rest of the app. Prints the highest macOS version the binaries need: the Homebrew
# bottles' (the macOS they were built for).
import glob
import os
import re
import shutil
import subprocess
import sys

app, prefix = sys.argv[1], os.path.realpath(sys.argv[2])
contents = os.path.join(app, "Contents")
helpers = os.path.join(contents, "Helpers")
frameworks = os.path.join(contents, "Frameworks")
EXECUTABLES = ["mpv", "ffmpeg", "ffprobe"]
# Python's extension modules that link libraries nothing here needs (OpenSSL, SQLite, Tk, readline...)
DROP_MODULES = re.compile(r"^(_tkinter|_ssl|_hashlib|_sqlite3|readline|_curses|_curses_panel|_dbm|_gdbm|"
                          r"_test\w*|_ctypes_test|xxlimited\w*|xxsubtype)\.")
DROP_STDLIB = ["test", "idlelib", "tkinter", "turtledemo", "ensurepip", "lib2to3", "pydoc_data", "venv",
               "sqlite3", "curses", "dbm"]
# Of the vapoursynth package (VapourSynth 74 and later): what's for building plugins, and the plugins folder,
# a link into <prefix>
DROP_VAPOURSYNTH = ["include", "pkgconfig", "plugins", "vspipe"]


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def is_macho(path):
    if os.path.islink(path) or not os.path.isfile(path):
        return False
    with open(path, "rb") as f:
        return f.read(4) in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")


def deps(path):
    lines = run("/usr/bin/otool", "-L", path).splitlines()[1:]
    return [line.strip().split(" (compatibility")[0] for line in lines]


def rpaths(path):
    out = run("/usr/bin/otool", "-l", path).splitlines()
    return [out[i + 2].split()[1] for i, line in enumerate(out) if "LC_RPATH" in line]


def dylib_id(path):
    out = run("/usr/bin/otool", "-D", path).splitlines()
    return out[1] if len(out) > 1 else None


def resolve(dep, origin):
    """Where a library a binary links is, from the binary's original place in <prefix>"""
    here = os.path.dirname(origin)
    if dep.startswith("@loader_path/") or dep.startswith("@executable_path/"):
        return os.path.realpath(os.path.join(here, dep.split("/", 1)[1]))
    if dep.startswith("@rpath/"):
        for rp in rpaths(origin):
            rp = rp.replace("@loader_path", here).replace("@executable_path", here)
            p = os.path.join(rp, dep[len("@rpath/"):])
            if os.path.exists(p):
                return os.path.realpath(p)
        raise SystemExit(f"{origin}: can't find {dep}")
    return os.path.realpath(dep)


def ours(path):
    return path.startswith(prefix + "/") or "/Cellar/" in path or path.startswith("/opt/homebrew/")


def find_vapoursynth():
    """The vapoursynth module, and the Python it's for: Homebrew's, or VapourSynth's venv of it (74 and later)"""
    candidates = [os.path.join(prefix, "opt/vapoursynth/libexec/bin/python3")]
    candidates += sorted(glob.glob(os.path.join(prefix, "opt/python@3.*/bin/python3.*[0-9]")), reverse=True)
    for py in candidates:
        if not os.path.exists(py):
            continue
        r = subprocess.run([py, "-c", "import sys, vapoursynth; print(vapoursynth.__file__); "
                                      "print(sys.base_prefix); print('%d.%d' % sys.version_info[:2])"],
                           capture_output=True, text=True)
        if r.returncode == 0:
            module, base, version = r.stdout.split("\n")[:3]
            return os.path.realpath(module), os.path.realpath(base), version
    raise SystemExit("No Python in " + prefix + " imports vapoursynth")


# Homebrew's builds the app mustn't carry: what `brew upgrade` would fix stays in the app
MIN_KEGS = {
    # mpv crashed when the audio device changed, headphones plugged in while it plays (Homebrew's backport)
    "mpv": "0.41.0_10",
}


def keg_version(v):
    """0.41.0_10 as ((0, 41, 0), 10), for comparing"""
    version, _, revision = v.partition("_")
    return tuple(int(x) for x in re.findall(r"\d+", version)), int(revision or 0)


for name, least in MIN_KEGS.items():
    m = re.search(rf"/Cellar/{re.escape(name)}/([^/]+)/", os.path.realpath(os.path.join(prefix, "bin", name)))
    if not m or keg_version(m.group(1)) < keg_version(least):
        raise SystemExit(f"Homebrew's {name} is {m and m.group(1)}: the app needs {least} or later "
                         f"(brew update && brew upgrade {name})")

vs_module, src_ver, py_version = find_vapoursynth()
py_fw = os.path.join(frameworks, "Python.framework")
py_ver = os.path.join(py_fw, "Versions", py_version)
stdlib = os.path.join(py_ver, "lib", f"python{py_version}")
site_packages = os.path.join(stdlib, "site-packages")

for d in (helpers, frameworks, os.path.join(contents, "Resources", "vulkan", "icd.d")):
    os.makedirs(d, exist_ok=True)

shutil.rmtree(py_fw, ignore_errors=True)
os.makedirs(os.path.join(py_ver, "bin"))
shutil.copy2(os.path.join(src_ver, "Python"), py_ver)
# With Python.app: bin/python3 (the setup's) runs the interpreter in it
shutil.copytree(os.path.join(src_ver, "Resources"), os.path.join(py_ver, "Resources"), symlinks=True)
shutil.copy2(os.path.join(src_ver, "bin", f"python{py_version}"), os.path.join(py_ver, "bin"))
os.symlink(f"python{py_version}", os.path.join(py_ver, "bin", "python3"))
shutil.copytree(os.path.join(src_ver, "lib", f"python{py_version}"), stdlib, symlinks=True,
                ignore=shutil.ignore_patterns(*DROP_STDLIB, "config-*", "site-packages", "*.pyc.*"))
for f in os.listdir(os.path.join(stdlib, "lib-dynload")):
    if DROP_MODULES.match(f):
        os.remove(os.path.join(stdlib, "lib-dynload", f))
os.makedirs(site_packages)
# vapoursynth.so alone (up to 73), or the vapoursynth package, with VapourSynth's libraries in it
if os.path.basename(vs_module) == "__init__.py":
    vs_src = os.path.dirname(vs_module)
    vs_dst = os.path.join(site_packages, "vapoursynth")
    shutil.copytree(vs_src, vs_dst, symlinks=True, ignore=shutil.ignore_patterns(*DROP_VAPOURSYNTH))
else:
    vs_src = vs_module
    vs_dst = os.path.join(site_packages, os.path.basename(vs_module))
    shutil.copy2(vs_module, vs_dst)
os.symlink(py_version, os.path.join(py_fw, "Versions", "Current"))
for name in ("Python", "Resources"):
    os.symlink(f"Versions/Current/{name}", os.path.join(py_fw, name))

# What the copies were copied from, to resolve the libraries they link from there
origin = {}
for exe in EXECUTABLES:
    dst = os.path.join(helpers, exe)
    src = os.path.realpath(os.path.join(prefix, "bin", exe))
    shutil.copy2(src, dst)
    origin[dst] = src
molten = os.path.join(prefix, "opt/molten-vk/lib/libMoltenVK.dylib")
shutil.copy2(os.path.realpath(molten), os.path.join(frameworks, "libMoltenVK.dylib"))
origin[os.path.join(frameworks, "libMoltenVK.dylib")] = os.path.realpath(molten)
with open(os.path.join(contents, "Resources", "vulkan", "icd.d", "MoltenVK_icd.json"), "w") as f:
    f.write('{\n    "file_format_version": "1.0.0",\n    "ICD": {\n'
            '        "library_path": "../../../Frameworks/libMoltenVK.dylib",\n'
            '        "api_version": "1.4.0",\n        "is_portability_driver": true\n    }\n}\n')
for root, _, files in os.walk(py_ver):
    for f in files:
        p = os.path.join(root, f)
        if not is_macho(p):
            continue
        if p == vs_dst or p.startswith(vs_dst + "/"):
            origin[p] = os.path.realpath(vs_src + p[len(vs_dst):])
        else:
            origin[p] = os.path.realpath(os.path.join(src_ver, os.path.relpath(p, py_ver)))
if os.path.isdir(vs_dst) and os.path.exists(os.path.join(vs_dst, "_cli.py")):
    vs_command = os.path.join(contents, "Resources", "bin", "vapoursynth")
    os.makedirs(os.path.dirname(vs_command), exist_ok=True)
    with open(vs_command, "w") as f:
        f.write('#!/bin/sh\n# VSScript runs `vapoursynth config` to find its Python when it has none in '
                '~/.config/vapoursynth/vapoursynth.toml yet: this app\'s\n'
                f'exec "$(dirname "$0")/../../Frameworks/Python.framework/Versions/{py_version}/bin/python3" -c '
                '"import sys; from vapoursynth._cli import main; sys.exit(main())" "$@"\n')
    os.chmod(vs_command, 0o755)

# The libraries, transitively: those not copied yet into Frameworks/, under the name they're linked by
copy_of = {src: dst for dst, src in origin.items()}
queue = list(origin)
while queue:
    path = queue.pop()
    own_id = dylib_id(path)
    for dep in deps(path):
        if dep == own_id or not (ours(dep) or dep.startswith("@")):
            continue
        src = resolve(dep, origin[path])
        if src in copy_of or not ours(src):
            continue
        dst = os.path.join(frameworks, os.path.basename(dep))
        if dst in origin:
            raise SystemExit(f"Two libraries named {os.path.basename(dep)}: {origin[dst]}, {src}")
        shutil.copy2(src, dst)
        os.chmod(dst, 0o755)
        origin[dst] = src
        copy_of[src] = dst
        queue.append(dst)

# Relinked: each library by its place relative to the binary, an rpath to Frameworks/ for what's opened by name
minos = []
for path in origin:
    os.chmod(path, os.stat(path).st_mode | 0o200)
    args = []
    own_id = dylib_id(path)
    if own_id:
        args += ["-id", "@rpath/" + os.path.relpath(path, frameworks)]
    for dep in deps(path):
        if dep == own_id or not (ours(dep) or dep.startswith("@")):
            continue
        target = copy_of.get(resolve(dep, origin[path]))
        if target:
            args += ["-change", dep, "@loader_path/" + os.path.relpath(target, os.path.dirname(path))]
    for rp in rpaths(path):
        args += ["-delete_rpath", rp]
    args += ["-add_rpath", "@loader_path/" + os.path.relpath(frameworks, os.path.dirname(path))]
    subprocess.run(["/usr/bin/install_name_tool", *args, path], check=True, capture_output=True)
    for line in run("/usr/bin/otool", "-l", path).splitlines():
        if line.strip().startswith("minos"):
            minos.append(tuple(int(x) for x in line.split()[1].split(".")))

# Nothing left that points into <prefix>
for path in origin:
    left = [d for d in deps(path) if ours(d)]
    if left:
        raise SystemExit(f"{path} still links {left}")

# Their licenses and where their sources are (the GPL's and LGPL's terms for passing their binaries on): for each
# Homebrew formula something here came from, its license files, and its formula, which has the source's URL, its
# checksum, the patches and the build's options
ffmpeg_conf = run(os.path.join(prefix, "bin", "ffmpeg"), "-hide_banner", "-buildconf")
if "--enable-nonfree" in ffmpeg_conf:
    raise SystemExit("Homebrew's ffmpeg is built with --enable-nonfree: it can't be passed on")
kegs = {}
for src in origin.values():
    m = re.search(r"/Cellar/([^/]+)/([^/]+)/", src)
    if m:
        kegs[m.group(1)] = m.group(2)
info = {f["name"]: f for f in __import__("json").loads(
    run(os.path.join(prefix, "bin", "brew"), "info", "--json=v2", *sorted(kegs)))["formulae"]}
third = os.path.join(contents, "Resources", "ThirdParty")
shutil.rmtree(third, ignore_errors=True)
os.makedirs(third)
lines = []
for name, version in sorted(kegs.items()):
    keg = os.path.join(prefix, "Cellar", name, version)
    dst = os.path.join(third, f"{name}-{version}")
    os.makedirs(dst)
    for f in os.listdir(keg):
        license_file = re.match(r"(?i)(licen[cs]e|copying|copyright|notice|authors|sbom)", f)
        if license_file and os.path.isfile(os.path.join(keg, f)):
            shutil.copy2(os.path.join(keg, f), dst)
    # The source this keg was built from: its own formula's (Homebrew's current one may be a later version)
    formula = os.path.join(keg, ".brew", f"{name}.rb")
    url = sha = None
    if os.path.exists(formula):
        shutil.copy2(formula, os.path.join(dst, f"{name}.rb"))
        rb = open(formula).read()
        m = re.search(r'^\s*url "([^"]+)"', rb, re.M)
        url = m and m.group(1)
        m = re.search(r'^\s*sha256 "([0-9a-f]{64})"', rb, re.M)
        sha = m and m.group(1)
    lines.append(f"{name} {version}\n  license: {info.get(name, {}).get('license')}\n"
                 f"  source:  {url}\n  sha256:  {sha}\n"
                 f"  home:    {info.get(name, {}).get('homepage')}\n")
with open(os.path.join(third, "SOURCES.txt"), "w") as f:
    f.write("ActionFootagePlayer carries these programs and libraries, as Homebrew (https://brew.sh) builds them,\n"
            "each under its own license (its folder here has the license files and the Homebrew formula it was\n"
            "built by: the source, its checksum, the patches and the build's options). The sources of these exact\n"
            "versions are at the URLs below; on request, they're also available from the ActionFootagePlayer\n"
            "project (https://github.com/anton-belkin/ActionFootagePlayer).\n\n"
            "ffmpeg's build: " + " ".join(ffmpeg_conf.split()) + "\n\n" + "\n".join(lines))

# Signed inside out: the libraries and binaries, then Python.app and Python.framework
for path in sorted(origin, key=lambda p: -p.count("/")):
    subprocess.run(["/usr/bin/codesign", "--force", "-s", "-", path], check=True, capture_output=True)
for path in (os.path.join(py_ver, "Resources", "Python.app"), py_ver):
    subprocess.run(["/usr/bin/codesign", "--force", "-s", "-", path], check=True, capture_output=True)

print(".".join(str(x) for x in max(minos)))
