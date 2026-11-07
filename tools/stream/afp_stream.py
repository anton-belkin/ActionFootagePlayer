#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# afp-stream: clips stabilized and in HDR, as HLS streams over the network (docs/gpu-render.md, "The streaming server").
# Runs on the NAS (tools/stream/README.md): for each clip in the library folders, an HLS stream (HEVC Main10, HLG,
# fMP4 segments) rendered on the GPU by afp-render as it's asked for, with the Gyroflow warp gyroflow-gpu-table writes
# for it (the clip's project, else its own motion data with the underwater preset) and the D-Log M look for DJI log clips.
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

jobs = {}  # key -> {proc, dir, started}
lock = threading.Lock()


def clips():
    out = []
    for root in LIBRARY:
        for d, _, files in os.walk(root):
            if "/_" in d or "/." in d or "#recycle" in d:
                continue
            for f in sorted(files):
                if VIDEO.search(f) and not f.startswith("."):
                    path = os.path.join(d, f)
                    out.append({"id": hashlib.sha1(path.encode()).hexdigest()[:12], "path": path, "name": os.path.relpath(path, root)})
    return out


def probe(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=width,height,pix_fmt,color_transfer,r_frame_rate:format=duration", "-of", "json", path], capture_output=True, text=True)
    j = json.loads(r.stdout or "{}")
    s = (j.get("streams") or [{}])[0]
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
    """The clip's size, rate and length, and its segments: a GOP of 2 s worth of frames each (afp-render's), the last
    shorter. The VOD playlist lists them all before any is rendered"""
    if clip["id"] not in infos:
        s, duration = probe(clip["path"])
        num, _, den = (s.get("r_frame_rate") or "30/1").partition("/")
        fps = float(num) / float(den or 1) if float(den or 1) else 30.0
        gop = int(fps * SEGMENT + 0.5)
        seg = gop / fps
        infos[clip["id"]] = {"stream": s, "duration": duration, "fps": fps, "seg": seg, "count": max(1, math.ceil(duration / seg - 1e-6))}
    return infos[clip["id"]]


def playlist(clip):
    i = info(clip)
    lines = ["#EXTM3U", "#EXT-X-VERSION:7", f"#EXT-X-TARGETDURATION:{math.ceil(i['seg'])}", "#EXT-X-MEDIA-SEQUENCE:0",
             "#EXT-X-PLAYLIST-TYPE:VOD", "#EXT-X-INDEPENDENT-SEGMENTS", '#EXT-X-MAP:URI="init.mp4"']
    for n in range(i["count"]):
        d = min(i["seg"], i["duration"] - n * i["seg"]) if n == i["count"] - 1 else i["seg"]
        lines += [f"#EXTINF:{max(d, 0.001):.6f},", f"seg{n:05d}.m4s"]
    lines.append("#EXT-X-ENDLIST")
    return "\n".join(lines) + "\n"


def segment_path(clip, n):
    return os.path.join(CACHE, clip["id"], f"seg{n:05d}.m4s")


def highest(clip, since):
    """The last segment the running render has finished, from `since` on"""
    n = since
    while os.path.exists(segment_path(clip, n)):
        n += 1
    return n - 1


def shader_for(clip, out, s):
    """The warp for the clip, written once: its project, else its own motion data with the underwater preset"""
    shader = os.path.join(out, "warp.glsl")
    meta = os.path.join(out, "warp.json")
    if os.path.exists(meta):
        with open(meta) as f:
            return json.load(f)
    w, h = int(s.get("width") or 3840), int(s.get("height") or 2160)
    project = project_of(clip["path"])
    ud = {"project": project or clip["path"], "input_width": w, "input_height": h, "interpolation": INTERP,
          "gpu_chromatic": CHROMATIC, "edges": "smooth", "edges_blur": 2, "underwater": "project" if project else "auto",
          "level": "medium", "lookahead": 0}
    if not project:
        ud["preset"] = os.path.join(BIN, "underwater.gyroflow")
    def table(ud):
        r = subprocess.run([os.path.join(BIN, "gyroflow-gpu-table"), "--player", json.dumps(ud), shader], capture_output=True, text=True)
        try:
            return json.loads(r.stdout.strip().splitlines()[-1])
        except Exception:
            return {"error": (r.stderr or r.stdout)[-300:]}
    reply = table(ud)
    if not reply.get("shader") and "no motion data" in str(reply.get("error", "")):
        # No motion data (a camera's own stabilization on): the lens alone corrected, as the player does
        reply = table(dict(ud, lens="only"))
    if not reply.get("shader"):
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
    """What a clip's renders depend on: when it changes (the project saved in Gyroflow, the service's settings), what was
    rendered goes"""
    def stamp(p):
        try:
            st = os.stat(p)
            return f"{st.st_size}:{int(st.st_mtime)}"
        except OSError:
            return "-"
    return f"v3|{SEGMENT}|{INTERP}|{CHROMATIC}|{HEIGHT}|{BITRATE}|{stamp(clip['path'])}|{stamp(project_of(clip['path']) or '')}"


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
    reply = shader_for(clip, out, s)
    shaders = [reply["shader"]] if reply.get("shader") else []
    ow, oh = reply["output"]
    height = min(HEIGHT, oh)
    width = int(round(height * ow / oh / 2)) * 2
    args = [os.path.join(BIN, "afp-render")]
    # DJI's D-Log M (10-bit, not tagged HLG or PQ): its look to HLG, as the player's hdr look
    if os.path.basename(clip["path"]).startswith("DJI_") and "10" in (s.get("pix_fmt") or "") and s.get("color_transfer") not in ("arib-std-b67", "smpte2084"):
        shaders.append(os.path.join(BIN, "dlogm_hlg.glsl"))
        args.append("--hlg")
    args += ["-w", str(width), "-h", str(height), "-b", BITRATE, "-seg", str(SEGMENT), "-ss", f"{n * i['seg']:.6f}", "-start_number", str(n)]
    if upto is not None:
        args += ["-t", f"{(upto - n) * i['seg'] - 0.5 / i['fps']:.6f}"]
    args += [clip["path"], out] + shaders
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
            self.wfile.write(data)

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
                    done = len([f for f in os.listdir(os.path.join(CACHE, c["id"])) if f.startswith("seg")]) if os.path.isdir(os.path.join(CACHE, c["id"])) else 0
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
        if len(parts) == 2 and parts[0] == "watch":
            return self.send(200, f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><body style='margin:0;background:#000'>"
                                  f"<video src='/clip/{parts[1]}/index.m3u8' controls autoplay playsinline muted style='width:100vw;height:100vh'></video>", "text/html")
        if len(parts) == 3 and parts[0] == "clip":
            clip = next((c for c in clips() if c["id"] == parts[1]), None)
            if not clip:
                return self.send(404, "no such clip", "text/plain")
            name = os.path.basename(parts[2])
            if name == "index.m3u8":
                # The whole clip's segments at once (VOD); the render starts from the first missing of the first few
                first = next((k for k in range(min(6, info(clip)["count"])) if not os.path.exists(segment_path(clip, k))), None)
                if first is not None:
                    ensure(clip, first)
                return self.send(200, playlist(clip), "application/vnd.apple.mpegurl")
            m = re.match(r"seg(\d+)\.m4s$", name)
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
                if not wait_for(path, j):
                    return self.send(503, "not rendered", "text/plain", {"Retry-After": "2"})
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
        pass


if __name__ == "__main__":
    os.makedirs(CACHE, exist_ok=True)
    log(f"afp-stream on :{PORT}, library {LIBRARY}, {len(clips())} clips")
    ThreadingHTTPServer(("", PORT), Handler).serve_forever()
