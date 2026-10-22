#!/usr/bin/python3 -I
# default-app.py <content type> [<bundle id>]: the default app for a content type (e.g. public.mpeg-4), for all
# roles: prints its bundle id, or makes the app given the default. Steady Player's setup.sh and install.sh use it.

import ctypes, sys, time

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
    # LaunchServices applies it asynchronously: wait, so whatever runs next sees it
    for _ in range(150):
        if (default_handler(content_type) or "").lower() == bundle_id.lower():
            return
        time.sleep(0.1)
    print(f"(macOS hasn't confirmed {content_type} -> {bundle_id} yet)", file=sys.stderr)

if len(sys.argv) == 2:
    print(default_handler(sys.argv[1]) or "")
elif len(sys.argv) == 3:
    set_default_handler(sys.argv[1], sys.argv[2])
else:
    sys.exit("usage: default-app.py <content type> [<bundle id>]")
