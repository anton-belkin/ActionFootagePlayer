#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# afp-stream: clips stabilized and in HDR, as HLS streams over the network (docs/gpu-render.md, "The streaming server").
# Runs on the NAS (tools/stream/README.md): for each clip in the library folders, an HLS stream (HEVC Main10, HLG,
# fMP4 segments) rendered on the GPU by afp-render as it's asked for, with the Gyroflow warp gyroflow-gpu-table writes
# for it as the player plays it (below) and the D-Log M look for D-Log M clips.
#
#   GET /                      the clips, each with its stream (Safari plays HLS by itself)
#   GET /playlist.m3u          every clip as a channel: Jellyfin's Live TV takes it as an M3U tuner
#   GET /clip/<id>/index.m3u8  the clip's stream as VOD: every segment listed at once (from the clip's rate and length),
#                              the render started from the first
#   GET /clip/<id>/<file>      its init segment and segments: a request waits until it's rendered; one far ahead of the
#                              render (a seek) starts it again from there. What played stays (AFP_CACHE_GB)
#   GET /prerender?folder=<f>&what=opening|whole   a folder's clips queued to be rendered in the background (their first
#                              AFP_OPENING seconds, or all), one at a time, while nobody watches
#   GET /status                what renders and what's queued
#   GET /catalog.json          the project's moments and clip previews (_Moments/, _Clips/), each as the player plays it:
#                              its name, part, date and stream, for tools/stream/jellyfin.py
#   GET /poster/<id>.jpg       a moment's or clip preview's cover (the frame its file shows)
#
# A moment's or clip preview's file (.afplink.mp4) is a stream of its own: the part of the original it links to, with
# the settings it plays with in the player. The player itself says what those are: mpv with the player's own plugins
# (AFP_PLAYER: workspace, gyroflow, dlogm and their config/script-opts), without a window, asked with the gyroflow
# plugin's render_args. So a clip's saved view, a moment's own, its Gyroflow project and the defaults all count, as in
# the player, and a change to any of them renders it again. Originals too (a clip with a saved view)
#
# Environment: AFP_LIBRARY (folders, ':'-separated; default /clips), AFP_CACHE (/stream/cache), AFP_BIN (/stream: afp-render,
# gyroflow-gpu-table, the shaders), AFP_PORT (8099), AFP_HEIGHT (1080), AFP_BITRATE (12M), AFP_CHROMATIC (0; 1: the water's
# color fringes taken out, the RGB layout: 48 fps on the N100, not real time at 4K60), AFP_INTERP (bilinear, bspline,
# bicubic, lanczos)
import hashlib, html, json, math, os, re, shutil, subprocess, threading, time, urllib.parse
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler

LIBRARY = [p for p in os.environ.get("AFP_LIBRARY", "/clips").split(":") if p]
CACHE = os.environ.get("AFP_CACHE", "/stream/cache")
BIN = os.environ.get("AFP_BIN", "/stream")
PORT = int(os.environ.get("AFP_PORT", "8099"))
HEIGHT = int(os.environ.get("AFP_HEIGHT", "1080"))
BITRATE = os.environ.get("AFP_BITRATE", "12M")
# The water's color fringes taken out (the RGB layout: the whole source converted first, slower), or not (the planes)
CHROMATIC = os.environ.get("AFP_CHROMATIC", "0") == "1"
# The warp's interpolation: bilinear plays 4K60 -> 1080p at 67 fps on the N100 (bspline 61, bicubic 59: no margin)
INTERP = os.environ.get("AFP_INTERP", "bilinear")
MAX_JOBS = 2
# Segments of a second: the first is there ~1 s after the render starts, and a player's buffer fills in small steps
SEGMENT = float(os.environ.get("AFP_SEGMENT", "1"))
# What played stays, up to this many GB of renders (the clips played longest ago go first)
CACHE_GB = float(os.environ.get("AFP_CACHE_GB", "40"))
# Pre-rendered openings (a folder's, on demand): this many seconds of each clip
OPENING = float(os.environ.get("AFP_OPENING", "8"))
VIDEO = re.compile(r"\.(mp4|mov)$", re.I)
LINK = re.compile(r"\.afplink\.mp4$", re.I)
# The player's plugins and settings (render_args), and what they said for each file
PLAYER = os.environ.get("AFP_PLAYER", "/stream/player")
ARGS = os.environ.get("AFP_ARGS", "/stream/args")

jobs = {}  # key -> {proc, dir, started}
lock = threading.Lock()


_clips = {"at": 0, "list": []}


def clips():
    """The library's videos: originals, and a project's moments (_Moments/) and clip previews (_Clips/), which link to
    them. Other folders starting with _ (exports, stills) and hidden ones aren't. Listed again every 10 s at most"""
    if time.time() - _clips["at"] < 10:
        return _clips["list"]
    out = []
    for root in LIBRARY:
        for d, dirs, files in os.walk(root):
            dirs[:] = sorted(x for x in dirs if not x.startswith(".") and x != "#recycle")
            rel = os.path.relpath(d, root)
            parts = [] if rel == "." else rel.split(os.sep)
            under = [p for p in parts if p.startswith("_")]
            kind = "clip"
            if under:
                if under[0] not in ("_Moments", "_Clips"):
                    continue
                kind = "moment" if under[0] == "_Moments" else "preview"
            for f in sorted(files):
                if f.startswith(".") or not VIDEO.search(f) or (kind == "clip") == bool(LINK.search(f)):
                    continue
                path = os.path.join(d, f)
                c = {"id": hashlib.sha1(path.encode()).hexdigest()[:12], "path": path, "name": os.path.relpath(path, root), "kind": kind}
                if kind != "clip":
                    # The project (the folder above _Moments/ or _Clips/) and the file's folder in there
                    i = parts.index(under[0])
                    c["project"] = os.path.join(root, *parts[:i]) if i else root
                    c["folder"] = "/".join(parts[i + 1:])
                out.append(c)
    _clips.update(at=time.time(), list=out)
    return out


def stamp(p):
    try:
        st = os.stat(p)
        return f"{st.st_size}:{int(st.st_mtime)}"
    except (OSError, TypeError):
        return "-"


def sidecars(path):
    """The files beside a clip, moment or clip preview that change how the player plays it: its saved view
    (<clip>.afp.json), its Gyroflow project (<name>.gyroflow, <file>.gyroflow)"""
    base = LINK.sub("", path) if LINK.search(path) else os.path.splitext(path)[0]
    return [base + ".afp.json", base + ".gyroflow", path + ".gyroflow"]


_args = {}
args_lock = threading.RLock()


def player_args(clip):
    """How the player plays the file (render_args): clip (the original), offset and length (a moment's part), render
    (user_data, gyroflow-gpu-table's request; none: not stabilized), dlogm (the log curve). Kept in ARGS with the stamps
    of the files it depended on, and asked again when one changed. None: the player couldn't (an original not found)"""
    a = _args.get(clip["id"])
    if a is None:
        try:
            with open(os.path.join(ARGS, clip["id"] + ".json")) as f:
                a = json.load(f)
        except (OSError, ValueError):
            a = None
    if a is not None and all(stamp(p) == s for p, s in a.get("deps", {}).items()):
        _args[clip["id"]] = a
        return a if a.get("clip") else None
    with args_lock:
        if _args.get(clip["id"]) is not a and _args.get(clip["id"]) is not None:
            return player_args(clip)
        return ask_player(clip)


def ask_player(clip):
    os.makedirs(ARGS, exist_ok=True)
    tmp = os.path.join(ARGS, clip["id"] + ".tmp.json")
    if os.path.exists(tmp):
        os.remove(tmp)
    args = ["mpv", "--no-config", f"--config-dir={PLAYER}/config", "--load-scripts=no"]
    args += [f"--script={os.path.join(PLAYER, p)}" for p in ("workspace", "gyroflow", "dlogm")]
    args += ["--vo=null", "--ao=null", "--hwdec=no", "--pause", "--autocreate-playlist=no", "--idle=no", "--msg-level=all=error",
             f"--script-opts-append=gyroflow-render_args={tmp}", "--", clip["path"]]
    t = time.time()
    try:
        # In ARGS: the workspace plugin's cache of moments (~~cache) lands there
        r = subprocess.run(args, capture_output=True, text=True, timeout=180, cwd=ARGS)
        err = r.stderr
    except subprocess.TimeoutExpired:
        err = "timed out"
    try:
        with open(tmp) as f:
            a = json.load(f)
        os.remove(tmp)
    except (OSError, ValueError):
        a = {"error": (err or "no reply")[-300:]}
    if a.get("moment", {}) and a["moment"].get("missing"):
        a = {"error": "the original isn't there"}
    deps = [clip["path"], os.path.join(PLAYER, "config", "script-opts", "gyroflow.conf"), os.path.join(PLAYER, "config", "script-opts", "dlogm.conf")]
    deps += sidecars(clip["path"])
    if a.get("clip"):
        deps += [a["clip"]] + sidecars(a["clip"])
        ud = (a.get("render") or {}).get("user_data") or {}
        if ud.get("project") and ud["project"] != a["clip"]:
            deps.append(ud["project"])
    a["deps"] = {p: stamp(p) for p in dict.fromkeys(deps)}
    log("player args for", clip["name"], f"in {time.time() - t:.1f} s:", a.get("error") or (a.get("render") or {}).get("kind", "not stabilized"))
    with open(os.path.join(ARGS, clip["id"] + ".json"), "w") as f:
        json.dump(a, f)
    _args[clip["id"]] = a
    return a if a.get("clip") else None


def probe(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=width,height,pix_fmt,color_transfer,r_frame_rate:format=duration:format_tags=creation_time", "-of", "json", path],
                       capture_output=True, text=True)
    j = json.loads(r.stdout or "{}")
    s = (j.get("streams") or [{}])[0]
    s["creation_time"] = ((j.get("format") or {}).get("tags") or {}).get("creation_time")
    return s, float((j.get("format") or {}).get("duration") or 0)


def project_of(path):
    base = os.path.splitext(path)[0]
    for p in (base + ".gyroflow", path + ".gyroflow"):
        if os.path.exists(p):
            return p
    return None


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


SCALE_PLANES = """//!HOOK LUMA
//!BIND HOOKED
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!DESC scaled luma
vec4 hook() { return HOOKED_tex(HOOKED_pos); }

//!HOOK CHROMA
//!BIND HOOKED
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!DESC scaled chroma
vec4 hook() { return HOOKED_tex(HOOKED_pos + vec2(0.25 / HOOKED_size.x, 0.0)); }
"""

infos = {}


def info(clip):
    """What plays: the original (a moment's or clip preview's), its part (offset, duration), size and rate, how the
    player plays it (args), and its segments: a GOP of SEGMENT s worth of frames each (afp-render's), the last shorter.
    The VOD playlist lists them all before any is rendered. None: the player can't play it"""
    a = player_args(clip)
    if a is None:
        return None
    key = json.dumps(a, sort_keys=True)
    old = infos.get(clip["id"])
    if old is None or old["key"] != key:
        s, duration = probe(a["clip"])
        offset = float(a.get("offset") or 0)
        if a.get("length"):
            duration = min(float(a["length"]), duration - offset)
        num, _, den = (s.get("r_frame_rate") or "30/1").partition("/")
        fps = float(num) / float(den or 1) if float(den or 1) else 30.0
        gop = int(fps * SEGMENT + 0.5)
        seg = gop / fps
        infos[clip["id"]] = {"key": key, "args": a, "source": a["clip"], "offset": offset, "stream": s, "duration": duration, "fps": fps,
                             "seg": seg, "count": max(1, math.ceil(duration / seg - 1e-6))}
    return infos[clip["id"]]


# Players that take HLS only in MPEG-TS segments: TVs before fMP4 HLS (Samsung's Tizen before 5, 2020; LG's webOS
# before 3.5). Jellyfin can't tell a TS stream from an fMP4 one (both are its "hls" container), so it hands them ours to
# play directly; they get the same segments remuxed to TS (HEVC with its parameter sets in band, the clip's time kept)
TS_PLAYERS = re.compile(r"Tizen [1-4]\.|SMART-TV.*Tizen [1-4]|Web0S|webOS", re.I)


def wants_ts(headers, query):
    if "ts" in (urllib.parse.parse_qs(query).get("fmt") or []):
        return True
    return bool(TS_PLAYERS.search(headers.get("User-Agent") or ""))


def playlist(clip, ts=False):
    i = info(clip)
    lines = ["#EXTM3U", "#EXT-X-VERSION:3" if ts else "#EXT-X-VERSION:7", f"#EXT-X-TARGETDURATION:{math.ceil(i['seg'])}",
             "#EXT-X-MEDIA-SEQUENCE:0", "#EXT-X-PLAYLIST-TYPE:VOD"]
    if not ts:
        lines += ["#EXT-X-INDEPENDENT-SEGMENTS", '#EXT-X-MAP:URI="init.mp4"']
    for n in range(i["count"]):
        d = min(i["seg"], i["duration"] - n * i["seg"]) if n == i["count"] - 1 else i["seg"]
        lines += [f"#EXTINF:{max(d, 0.001):.6f},", f"seg{n:05d}.{'ts' if ts else 'm4s'}"]
    lines.append("#EXT-X-ENDLIST")
    return "\n".join(lines) + "\n"


def ts_segment(clip, n):
    """Segment n remuxed to MPEG-TS (init + segment through ffmpeg, timestamps kept), made once beside it"""
    m4s = segment_path(clip, n)
    out = m4s[:-4] + ".ts"
    if os.path.exists(out) and os.path.getmtime(out) >= os.path.getmtime(m4s):
        with open(out, "rb") as f:
            return f.read()
    with open(os.path.join(CACHE, clip["id"], "init.mp4"), "rb") as f:
        data = f.read()
    with open(m4s, "rb") as f:
        data += f.read()
    r = subprocess.run(["ffmpeg", "-v", "error", "-f", "mp4", "-i", "pipe:0", "-map", "0", "-c", "copy", "-copyts", "-muxdelay", "0",
                        "-muxpreload", "0", "-f", "mpegts", "pipe:1"], input=data, capture_output=True)
    if r.returncode != 0 or not r.stdout:
        log("no TS for", clip["name"], n, r.stderr.decode(errors="replace")[-200:])
        return None
    with open(out + ".tmp", "wb") as f:
        f.write(r.stdout)
    os.replace(out + ".tmp", out)
    return r.stdout


def segment_path(clip, n):
    return os.path.join(CACHE, clip["id"], f"seg{n:05d}.m4s")


def highest(clip, since):
    """The last segment the running render has finished, from `since` on"""
    n = since
    while os.path.exists(segment_path(clip, n)):
        n += 1
    return n - 1


def shader_for(clip, out, i):
    """The warp for the clip, written once: what the player plays it with (render_args), with the stream's own speed
    settings (interpolation, color fringes) and the smooth edge fill (the sphere trail isn't on the GPU path's stream);
    else the planes just scaled"""
    shader = os.path.join(out, "warp.glsl")
    meta = os.path.join(out, "warp.json")
    if os.path.exists(meta):
        with open(meta) as f:
            return json.load(f)
    s = i["stream"]
    w, h = int(s.get("width") or 3840), int(s.get("height") or 2160)
    render = i["args"].get("render") or {}
    reply = {}
    if render.get("stabilize") and render.get("user_data"):
        ud = dict(render["user_data"])
        for k in ("plugin", "edges_control", "still_control", "workers", "fast", "width", "height", "gpu_denoise"):
            ud.pop(k, None)
        ud.update(input_width=ud.get("input_width") or w, input_height=ud.get("input_height") or h, interpolation=INTERP,
                  gpu_chromatic=bool(CHROMATIC and ud.get("gpu_chromatic")), edges="smooth" if ud.get("edges") == "sphere" else ud.get("edges", "smooth"))
        r = subprocess.run([os.path.join(BIN, "gyroflow-gpu-table"), "--player", json.dumps(ud), shader], capture_output=True, text=True)
        try:
            reply = json.loads(r.stdout.strip().splitlines()[-1])
        except Exception:
            reply = {"error": (r.stderr or r.stdout)[-300:]}
    if not reply.get("shader"):
        if render.get("stabilize"):
            log("no warp for", clip["name"], reply)
        # The planes scaled to the output's size as they are: libplacebo then never converts the whole source (a 4K
        # source through its conversion renders at ~43 fps on the N100, too slow for 60)
        with open(shader, "w") as f:
            f.write(SCALE_PLANES)
        reply = {"shader": shader, "mode": "scaled"}
    reply.setdefault("output", [w, h])
    with open(meta, "w") as f:
        json.dump(reply, f)
    return reply


def fingerprint(clip):
    """What a clip's renders depend on: when it changes (what the player plays it with, its project saved in Gyroflow,
    the service's settings), what was rendered goes"""
    i = info(clip)
    a = i["args"]
    what = json.dumps({k: a.get(k) for k in ("clip", "offset", "length", "render", "dlogm", "deps")}, sort_keys=True)
    return f"v4|{SEGMENT}|{INTERP}|{CHROMATIC}|{HEIGHT}|{BITRATE}|{hashlib.sha1(what.encode()).hexdigest()}"


def clip_dir(clip):
    """The clip's renders, as they are now (stale ones gone); marked used now, for the cache's limit"""
    out = os.path.join(CACHE, clip["id"])
    os.makedirs(out, exist_ok=True)
    fp, mark = fingerprint(clip), os.path.join(out, "fingerprint")
    old = open(mark).read() if os.path.exists(mark) else None
    if old != fp and not any(v["dir"] == out and v["proc"].poll() is None for v in jobs.values()):
        shutil.rmtree(out, ignore_errors=True)
        os.makedirs(out, exist_ok=True)
        with open(mark, "w") as f:
            f.write(fp)
    os.utime(out)
    return out


def trim_cache():
    """Over CACHE_GB: the clips used longest ago go, but none being rendered"""
    dirs = []
    for d in os.listdir(CACHE):
        p = os.path.join(CACHE, d)
        if os.path.isdir(p):
            size = sum(os.path.getsize(os.path.join(p, f)) for f in os.listdir(p) if os.path.isfile(os.path.join(p, f)))
            dirs.append((os.path.getmtime(p), size, p))
    total = sum(s for _, s, _ in dirs)
    for _, size, p in sorted(dirs):
        if total <= CACHE_GB * 1e9:
            break
        if any(v["dir"] == p and v["proc"].poll() is None for v in jobs.values()):
            continue
        shutil.rmtree(p, ignore_errors=True)
        total -= size
        log("cache: dropped", os.path.basename(p))


def start_render(clip, n, upto=None, background=False):
    """afp-render from segment n (to segment `upto`, else the end) into the clip's folder"""
    out = clip_dir(clip)
    i = info(clip)
    s = i["stream"]
    reply = shader_for(clip, out, i)
    shaders = [reply["shader"]] if reply.get("shader") else []
    ow, oh = reply["output"]
    height = min(HEIGHT, oh)
    width = int(round(height * ow / oh / 2)) * 2
    args = [os.path.join(BIN, "afp-render")]
    # D-Log M, as the player's dlogm plugin takes it (its curve, by the camera's metadata or set by hand for the
    # source): its look to HLG, as the player's hdr look. Other log curves have no look here yet: as recorded
    curve = (i["args"].get("dlogm") or {}).get("curve") or ""
    if curve == "dlogm":
        shaders.append(os.path.join(BIN, "dlogm_hlg.glsl"))
        args.append("--hlg")
    elif curve:
        log("no look for the curve", curve, "of", clip["name"])
    # A moment's part: the clip's time from its in; the stream's time from 0, as mpv plays a moment
    off = i["offset"]
    args += ["-w", str(width), "-h", str(height), "-b", BITRATE, "-seg", str(SEGMENT), "-ss", f"{off + n * i['seg']:.6f}", "-start_number", str(n)]
    end = upto * i["seg"] if upto is not None else (i["duration"] if off or i["args"].get("length") else None)
    if end is not None:
        args += ["-t", f"{min(end, i['duration']) - n * i['seg'] - 0.5 / i['fps']:.6f}"]
    if off:
        args += ["-offset", f"{off:.6f}"]
    args += [i["source"], out] + shaders
    logf = open(os.path.join(out, "render.log"), "a")
    proc = subprocess.Popen(args, stdout=logf, stderr=subprocess.STDOUT, env=dict(os.environ, LD_LIBRARY_PATH="/opt/pl/lib"),
                            preexec_fn=(lambda: os.nice(10)) if background else None)
    log("rendering", clip["name"], "from segment", n, "to", upto if upto is not None else "the end", ":", reply.get("mode", "no warp"),
        "(in the background)" if background else "")
    j = {"proc": proc, "dir": out, "started": time.time(), "from": n, "background": background, "clip": clip["id"]}
    jobs[clip["id"]] = j
    trim_cache()
    return j


def ensure(clip, n):
    """A render that has segment n or will have it soon: the running one when it's at most a few segments short of n,
    else a new one from n (a seek), the old one stopped. Someone watching comes first: a background render stops"""
    with lock:
        out = clip_dir(clip)
        j = jobs.get(clip["id"])
        if os.path.exists(segment_path(clip, n)):
            return j
        if j and j["proc"].poll() is None and j["from"] <= n <= highest(clip, j["from"]) + 6 and not j["background"]:
            return j
        if j and j["proc"].poll() is None:
            j["proc"].terminate()
        for v in jobs.values():
            if v["background"] and v["proc"].poll() is None:
                v["proc"].terminate()
        running = sorted((v for v in jobs.values() if v["proc"].poll() is None), key=lambda v: v["started"])
        while len(running) >= MAX_JOBS:
            running.pop(0)["proc"].terminate()
        # From n to the end, skipping what's rendered: up to the next segment that's there, then on from there later
        upto = n
        count = info(clip)["count"]
        while upto < count and not os.path.exists(segment_path(clip, upto)):
            upto += 1
        return start_render(clip, n, upto if upto < count else None)


# ---- Pre-rendering on demand, per folder ----
queue = []  # [{"clip", "what": "opening" | "whole"}]
current = {"name": None}


def interactive_running():
    return any(not v["background"] and v["proc"].poll() is None for v in jobs.values())


def prerender_worker():
    """One clip at a time, at low priority, and only while nobody's watching: the missing segments of its opening (or of
    all of it), from the first missing one"""
    while True:
        time.sleep(1)
        if not queue or interactive_running():
            continue
        with lock:
            if not queue or interactive_running():
                continue
            item = queue[0]
            clip = item["clip"]
            i = info(clip)
            if i is None:
                queue.pop(0)
                continue
            last = i["count"] if item["what"] == "whole" else min(i["count"], math.ceil(OPENING / i["seg"]))
            clip_dir(clip)
            first = next((k for k in range(last) if not os.path.exists(segment_path(clip, k))), None)
            if first is None:
                queue.pop(0)
                continue
            upto = first
            while upto < last and not os.path.exists(segment_path(clip, upto)):
                upto += 1
            current["name"] = clip["name"]
            j = start_render(clip, first, upto if upto < i["count"] else None, background=True)
        j["proc"].wait()
        current["name"] = None


threading.Thread(target=prerender_worker, daemon=True).start()


def folder_of(clip):
    return os.path.dirname(clip["name"]) or "."


def wait_for(path, j, timeout=60):
    t = time.time()
    while time.time() - t < timeout:
        if os.path.exists(path):
            return True
        if j is None or j["proc"].poll() not in (None, 0):
            return os.path.exists(path)
        time.sleep(0.1)
    return False


def local_start(i):
    """When the part starts, by the camera's clock: a DJI clip's name is its local time; else the file's creation time
    (UTC, as the file has it). None: unknown"""
    import datetime
    m = re.match(r"DJI_(\d{14})_", os.path.basename(i["source"]))
    t = None
    if m:
        t = datetime.datetime.strptime(m.group(1), "%Y%m%d%H%M%S")
    elif i["stream"].get("creation_time"):
        try:
            t = datetime.datetime.strptime(i["stream"]["creation_time"][:19], "%Y-%m-%dT%H:%M:%S")
        except ValueError:
            t = None
    return (t + datetime.timedelta(seconds=i["offset"])).strftime("%Y-%m-%dT%H:%M:%S") if t else None


def catalog_entry(c, i, base):
    a = i["args"]
    moment = a.get("moment") or {}
    name = LINK.sub("", os.path.basename(c["path"]))
    return {"id": c["id"], "kind": c["kind"], "project": c["project"], "folder": c["folder"], "name": name,
            "title": moment.get("name") or name, "clip": os.path.relpath(i["source"], c["project"]),
            "in": i["offset"], "duration": round(i["duration"], 3), "start": local_start(i),
            "stabilized": bool((a.get("render") or {}).get("stabilize")), "kind_of_stabilization": (a.get("render") or {}).get("kind"),
            "look": "hdr" if (a.get("dlogm") or {}).get("curve") == "dlogm" else "",
            "stream": f"{base}/clip/{c['id']}/index.m3u8", "poster": f"{base}/poster/{c['id']}.jpg"}


def poster(clip):
    """The frame a moment's or clip preview's file shows: its cover art, else its first frame. Kept in ARGS"""
    path = os.path.join(ARGS, clip["id"] + ".jpg")
    mark = path + ".stamp"
    if os.path.exists(path) and os.path.exists(mark) and open(mark).read() == stamp(clip["path"]):
        with open(path, "rb") as f:
            return f.read()
    os.makedirs(ARGS, exist_ok=True)
    for args in (["-map", "0:v:1", "-c", "copy"], ["-map", "0:v:0", "-frames:v", "1", "-q:v", "3"]):
        r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", clip["path"]] + args + ["-f", "image2", path], capture_output=True)
        if r.returncode == 0 and os.path.exists(path) and os.path.getsize(path) > 0:
            with open(mark, "w") as f:
                f.write(stamp(clip["path"]))
            with open(path, "rb") as f:
                return f.read()
    return None


def warm_worker():
    """The moments' and clip previews' player args, asked ahead (the catalog lists only those it has), again when a file
    they depend on changed; then their posters"""
    while True:
        for c in clips():
            if c["kind"] != "clip":
                try:
                    if player_args(c):
                        poster(c)
                except Exception as e:
                    log("warm:", c["name"], e)
        time.sleep(60)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def send(self, code, body, ctype, extra=None):
        data = body if isinstance(body, bytes) else body.encode()
        rng = self.headers.get("Range")
        m = re.match(r"bytes=(\d*)-(\d*)$", rng or "")
        if code == 200 and m and (m.group(1) or m.group(2)):
            a = int(m.group(1)) if m.group(1) else max(0, len(data) - int(m.group(2)))
            b = min(int(m.group(2)), len(data) - 1) if (m.group(1) and m.group(2)) else len(data) - 1
            extra = dict(extra or {}, **{"Content-Range": f"bytes {a}-{b}/{len(data)}"})
            data, code = data[a:b + 1], 206
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cache-Control", "no-cache")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            try:
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                # The player went (stopped, or skipped ahead): nothing to do
                self.close_connection = True

    def base(self):
        return f"http://{self.headers.get('Host') or 'localhost:%d' % PORT}"

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        parts = [p for p in u.path.split("/") if p]
        if not parts:
            folders = {}
            for c in clips():
                folders.setdefault(folder_of(c), []).append(c)
            status = (f"Pre-rendering {html.escape(current['name'])}, {len(queue)} clips queued" if current["name"] else
                      f"{len(queue)} clips queued for pre-rendering" if queue else "Nothing being pre-rendered")
            body = ""
            for folder, cs in sorted(folders.items()):
                q = urllib.parse.quote(folder)
                body += (f"<h2>{html.escape(folder)}</h2><p><a href='/prerender?folder={q}&what=opening'>pre-render openings</a> · "
                         f"<a href='/prerender?folder={q}&what=whole'>pre-render whole clips</a></p><ul>")
                for c in cs:
                    i = infos.get(c["id"])
                    done = len([f for f in os.listdir(os.path.join(CACHE, c["id"])) if f.startswith("seg") and f.endswith(".m4s")]) if os.path.isdir(os.path.join(CACHE, c["id"])) else 0
                    have = f" <small>({done} s rendered)</small>" if done else ""
                    body += f'<li><a href="/clip/{c["id"]}/index.m3u8">{html.escape(os.path.basename(c["name"]))}</a> <a href="/watch/{c["id"]}">watch</a>{have}</li>'
                body += "</ul>"
            return self.send(200, f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>Action Footage streams</title>"
                                  f"<body style='font-family:system-ui;background:#111;color:#ddd'><style>a{{color:#8cf}}</style><h1>Clips</h1><p>Stabilized, HDR (HLG). "
                                  f"<a href=/playlist.m3u>playlist.m3u</a> for Jellyfin Live TV. {status}.</p>{body}", "text/html; charset=utf-8")
        if parts == ["prerender"]:
            q = urllib.parse.parse_qs(u.query)
            folder, what = (q.get("folder") or [""])[0], (q.get("what") or ["opening"])[0]
            if what not in ("opening", "whole"):
                return self.send(400, "what: opening or whole", "text/plain")
            added = 0
            with lock:
                for c in clips():
                    if folder_of(c) == folder and not any(x["clip"]["id"] == c["id"] and x["what"] == what for x in queue):
                        queue.append({"clip": c, "what": what})
                        added += 1
            log("pre-render", what, folder, added, "clips")
            return self.send(303, "", "text/plain", {"Location": "/"})
        if parts == ["status"]:
            return self.send(200, json.dumps({"prerendering": current["name"], "queued": len(queue),
                                              "rendering": [{"clip": v["clip"], "from": v["from"], "background": v["background"]} for v in jobs.values() if v["proc"].poll() is None]}),
                             "application/json")
        if parts == ["playlist.m3u"]:
            lines = ["#EXTM3U"]
            for c in clips():
                lines.append(f'#EXTINF:-1 tvg-id="{c["id"]}" tvg-name="{c["name"]}" group-title="Action Footage",{c["name"]}')
                lines.append(f"{self.base()}/clip/{c['id']}/index.m3u8")
            return self.send(200, "\n".join(lines) + "\n", "audio/x-mpegurl")
        if parts == ["catalog.json"]:
            items, pending = [], 0
            for c in clips():
                if c["kind"] == "clip":
                    continue
                a = _args.get(c["id"])
                if a is None or any(stamp(p) != v for p, v in a.get("deps", {}).items()):
                    pending += 1
                    continue
                i = info(c) if a.get("clip") else None
                if i is None:
                    continue
                items.append(catalog_entry(c, i, self.base()))
            return self.send(200, json.dumps({"items": items, "pending": pending}, ensure_ascii=False), "application/json; charset=utf-8")
        if len(parts) == 2 and parts[0] == "poster" and parts[1].endswith(".jpg"):
            clip = next((c for c in clips() if c["id"] == parts[1][:-4]), None)
            data = poster(clip) if clip else None
            if not data:
                return self.send(404, "no poster", "text/plain")
            return self.send(200, data, "image/jpeg")
        if len(parts) == 2 and parts[0] == "watch":
            return self.send(200, f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><body style='margin:0;background:#000'>"
                                  f"<video src='/clip/{parts[1]}/index.m3u8' controls autoplay playsinline muted style='width:100vw;height:100vh'></video>", "text/html")
        if len(parts) == 3 and parts[0] == "clip":
            clip = next((c for c in clips() if c["id"] == parts[1]), None)
            if not clip or info(clip) is None:
                return self.send(404, "no such clip", "text/plain")
            name = os.path.basename(parts[2])
            if name == "index.m3u8":
                # The whole clip's segments at once (VOD); the render starts from the first missing of the first few
                first = next((k for k in range(min(6, info(clip)["count"])) if not os.path.exists(segment_path(clip, k))), None)
                if first is not None:
                    ensure(clip, first)
                # Jellyfin passing the stream on to a device (its /Videos/<id>/stream, for a device that doesn't fetch the
                # .strm's URL itself: a Samsung TV) sends our playlist as it is, from its own address, where the segments'
                # relative names lead nowhere. It gets a master playlist instead, pointing here: the device then fetches
                # the stream from us, with its own User-Agent, and gets TS or fMP4 as it plays
                if (self.headers.get("User-Agent") or "").startswith("Jellyfin-Server"):
                    i = info(clip)
                    w, h = (shader_for(clip, clip_dir(clip), i).get("output") or [1920, 1080])
                    height = min(HEIGHT, h)
                    width = int(round(height * w / h / 2)) * 2
                    master = ("#EXTM3U\n#EXT-X-VERSION:3\n"
                              f"#EXT-X-STREAM-INF:BANDWIDTH={int(float(BITRATE.rstrip('MmKk')) * (1e6 if BITRATE[-1] in 'Mm' else 1e3) * 1.2)},"
                              f"RESOLUTION={width}x{height},FRAME-RATE={i['fps']:.3f}\n{self.base()}/clip/{clip['id']}/index.m3u8\n")
                    return self.send(200, master, "application/vnd.apple.mpegurl")
                return self.send(200, playlist(clip, wants_ts(self.headers, u.query)), "application/vnd.apple.mpegurl")
            m = re.match(r"seg(\d+)\.(m4s|ts)$", name)
            if m:
                n = int(m.group(1))
                if n >= info(clip)["count"]:
                    return self.send(404, "past the end", "text/plain")
                j = ensure(clip, n)
                # A cached stretch (a pre-rendered opening, what played before) ends somewhere: the render picks up a
                # few segments before the player gets there
                ahead = next((k for k in range(n + 1, min(n + 6, info(clip)["count"])) if not os.path.exists(segment_path(clip, k))), None)
                if ahead is not None:
                    ensure(clip, ahead)
                path = segment_path(clip, n)
                if not wait_for(path, j) or (m.group(2) == "ts" and not os.path.exists(os.path.join(CACHE, clip["id"], "init.mp4"))):
                    return self.send(503, "not rendered", "text/plain", {"Retry-After": "2"})
                if m.group(2) == "ts":
                    data = ts_segment(clip, n)
                    if data is None:
                        return self.send(500, "no TS", "text/plain")
                    return self.send(200, data, "video/mp2t")
                with open(path, "rb") as f:
                    return self.send(200, f.read(), "video/iso.segment")
            if name == "init.mp4":
                # Only once a segment of the render is there: the muxer writes init.mp4 in place (no temporary file),
                # and a player that reads it half written fails (CoreMedia -16044)
                path = os.path.join(CACHE, clip["id"], "init.mp4")
                j = ensure(clip, 0) if not os.path.exists(path) else jobs.get(clip["id"])
                first = segment_path(clip, j["from"] if j else 0)
                if not wait_for(first, j) or not os.path.exists(path):
                    return self.send(503, "not rendered", "text/plain", {"Retry-After": "2"})
                with open(path, "rb") as f:
                    return self.send(200, f.read(), "video/mp4")
        return self.send(404, "not found", "text/plain")

    def log_message(self, fmt, *args):
        # Who asked for what (a player that stalls: did it come, which files, which answer); AFP_LOG_REQUESTS=0: none
        if os.environ.get("AFP_LOG_REQUESTS", "1") != "0":
            ua = (self.headers.get("User-Agent") or "-")[:60] if self.headers else "-"
            log(self.client_address[0], fmt % args, ua)


if __name__ == "__main__":
    os.makedirs(CACHE, exist_ok=True)
    log(f"afp-stream on :{PORT}, library {LIBRARY}, {len(clips())} clips")
    threading.Thread(target=warm_worker, daemon=True).start()
    ThreadingHTTPServer(("", PORT), Handler).serve_forever()
