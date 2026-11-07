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
#                              render (a seek) starts it again from there
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
        gop = int(fps * 2 + 0.5)
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


def ensure(clip, n):
    """A render that has segment n or will have it soon: the running one when it's at most a few segments short of n,
    else a new one from n (a seek), the old one stopped"""
    with lock:
        j = jobs.get(clip["id"])
        if os.path.exists(segment_path(clip, n)):
            return j
        if j and j["proc"].poll() is None and j["from"] <= n <= highest(clip, j["from"]) + 4:
            return j
        if j and j["proc"].poll() is None:
            j["proc"].terminate()
        # At most MAX_JOBS renders at once: the oldest running one stops
        running = sorted((v for v in jobs.values() if v["proc"].poll() is None), key=lambda v: v["started"])
        while len(running) >= MAX_JOBS:
            running.pop(0)["proc"].terminate()
        # Renders not asked for in 12 hours go
        for d in os.listdir(CACHE):
            p = os.path.join(CACHE, d)
            if d != clip["id"] and os.path.isdir(p) and time.time() - os.path.getmtime(p) > 12 * 3600 and not any(v["dir"] == p and v["proc"].poll() is None for v in jobs.values()):
                shutil.rmtree(p, ignore_errors=True)
        out = os.path.join(CACHE, clip["id"])
        os.makedirs(out, exist_ok=True)
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
        args += ["-w", str(width), "-h", str(height), "-b", BITRATE, "-ss", f"{n * i['seg']:.6f}", "-start_number", str(n),
                 clip["path"], out] + shaders
        logf = open(os.path.join(out, "render.log"), "a")
        proc = subprocess.Popen(args, stdout=logf, stderr=subprocess.STDOUT, env=dict(os.environ, LD_LIBRARY_PATH="/opt/pl/lib"))
        log("rendering", clip["name"], "from segment", n, ":", reply.get("mode", "no warp"))
        j = {"proc": proc, "dir": out, "started": time.time(), "from": n}
        jobs[clip["id"]] = j
        return j


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
            rows = "".join(f'<li><a href="/clip/{c["id"]}/index.m3u8">{html.escape(c["name"])}</a> '
                           f'<a href="/watch/{c["id"]}">watch</a></li>' for c in clips())
            return self.send(200, f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>Action Footage streams</title>"
                                  f"<body style='font-family:system-ui;background:#111;color:#ddd'><h1>Clips</h1><p>Stabilized, HDR (HLG). "
                                  f"<a href=/playlist.m3u>playlist.m3u</a> for Jellyfin Live TV.</p><ul>{rows}</ul>", "text/html; charset=utf-8")
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
                # The whole clip's segments at once (VOD); the render starts from the first
                ensure(clip, 0)
                return self.send(200, playlist(clip), "application/vnd.apple.mpegurl")
            m = re.match(r"seg(\d+)\.m4s$", name)
            if m:
                n = int(m.group(1))
                if n >= info(clip)["count"]:
                    return self.send(404, "past the end", "text/plain")
                j = ensure(clip, n)
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
