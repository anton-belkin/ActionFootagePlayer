#!/usr/bin/python3 -I
# gyroflow-router on [--fallback <app>] | off | status
#
# Router mode: Steady Router becomes the default app for MP4 and MOV, and opens each video with Steady Player
# when it's one of ours (D-Log M, or something to stabilize it with) and with the fallback player otherwise.
#   on     remembers the current default players, makes Steady Router the default. The fallback is the player that
#          was the default (for a router of ours, IINA if it's installed, else QuickTime), or --fallback
#          (an app name, path or bundle id)
#   off    gives MP4 and MOV back to the players that were the default before `on`
#   status what's the default now and where the router sends other videos
#
# The settings live in Steady Player's defaults domain (@PLAYER_ID@): fallback-<ext>, previous-<content type>.

import ctypes, subprocess, sys, os

PLAYER_ID = "@PLAYER_ID@"
ROUTER_ID = "@ROUTER_ID@"
# Routers of earlier versions of this setup, which aren't players to fall back to
OUR_ROUTERS = { ROUTER_ID, "com.antonbelkin.dji-router", "io.github.dji-router" }
TYPES = { "mp4": "public.mpeg-4", "mov": "com.apple.quicktime-movie" }

cf = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
ls = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/CoreServices.framework/CoreServices")
cf.CFStringCreateWithCString.restype = ctypes.c_void_p
cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint32]
cf.CFStringGetCString.restype = ctypes.c_bool
cf.CFStringGetCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
cf.CFRelease.argtypes = [ctypes.c_void_p]
ls.LSCopyDefaultRoleHandlerForContentType.restype = ctypes.c_void_p
ls.LSCopyDefaultRoleHandlerForContentType.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
ls.LSSetDefaultRoleHandlerForContentType.restype = ctypes.c_int32
ls.LSSetDefaultRoleHandlerForContentType.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p]
UTF8 = 0x08000100
ROLES_ALL = 0xFFFFFFFF

def cfstr(s):
    return cf.CFStringCreateWithCString(None, s.encode(), UTF8)

def default_handler(content_type):
    ref = ls.LSCopyDefaultRoleHandlerForContentType(cfstr(content_type), ROLES_ALL)
    if not ref:
        return None
    buf = ctypes.create_string_buffer(512)
    ok = cf.CFStringGetCString(ref, buf, len(buf), UTF8)
    cf.CFRelease(ref)
    return buf.value.decode() if ok else None

def set_default_handler(content_type, bundle_id):
    status = ls.LSSetDefaultRoleHandlerForContentType(cfstr(content_type), ROLES_ALL, cfstr(bundle_id))
    if status != 0:
        sys.exit(f"Couldn't make {bundle_id} the default for {content_type} (error {status})")

def defaults_read(key):
    r = subprocess.run(["defaults", "read", PLAYER_ID, key], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None

def defaults_write(key, value):
    subprocess.run(["defaults", "write", PLAYER_ID, key, "-string", value], check=True)

def app_installed(bundle_id):
    r = subprocess.run(["mdfind", f"kMDItemCFBundleIdentifier == '{bundle_id}'"], capture_output=True, text=True)
    return bool(r.stdout.strip())

def bundle_id_of(app):
    if "." in app and "/" not in app and not app.endswith(".app"):
        return app
    target = f'POSIX file "{os.path.abspath(app)}"' if "/" in app or app.endswith(".app") else f'"{app}"'
    r = subprocess.run(["osascript", "-e", f"id of application {target}"], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"No app {app}")
    return r.stdout.strip()

def fallback_for(current):
    if current and current not in OUR_ROUTERS and current != PLAYER_ID:
        return current
    return "com.colliderli.iina" if app_installed("com.colliderli.iina") else "com.apple.QuickTimePlayerX"

def name_of(bundle_id):
    r = subprocess.run(["osascript", "-e", f'name of application id "{bundle_id}"'], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else bundle_id

def on(args):
    chosen = None
    if args[:1] == ["--fallback"] and len(args) == 2:
        chosen = bundle_id_of(args[1])
    elif args:
        sys.exit("usage: gyroflow-router on [--fallback <app>]")
    for ext, content_type in TYPES.items():
        current = default_handler(content_type)
        if current != ROUTER_ID:
            defaults_write(f"previous-{content_type}", current or "")
        previous = defaults_read(f"previous-{content_type}") or current
        fallback = chosen or fallback_for(previous)
        defaults_write(f"fallback-{ext}", fallback)
        set_default_handler(content_type, ROUTER_ID)
        print(f".{ext}: Steady Router; videos that aren't ours open with {name_of(fallback)}")

def off(args):
    for ext, content_type in TYPES.items():
        previous = defaults_read(f"previous-{content_type}")
        if default_handler(content_type) != ROUTER_ID:
            print(f".{ext}: Steady Router isn't the default, left as it is")
            continue
        target = previous if previous and previous not in OUR_ROUTERS else fallback_for(previous)
        set_default_handler(content_type, target)
        print(f".{ext}: back to {name_of(target)}")

def status(args):
    for ext, content_type in TYPES.items():
        current = default_handler(content_type)
        line = f".{ext}: {name_of(current) if current else 'no default'}"
        if current == ROUTER_ID:
            line += f" (router mode; videos that aren't ours open with {name_of(defaults_read(f'fallback-{ext}') or 'com.apple.QuickTimePlayerX')})"
        print(line)

commands = { "on": on, "off": off, "status": status }
if len(sys.argv) < 2 or sys.argv[1] not in commands:
    sys.exit("usage: gyroflow-router on [--fallback <app>] | off | status")
commands[sys.argv[1]](sys.argv[2:])
