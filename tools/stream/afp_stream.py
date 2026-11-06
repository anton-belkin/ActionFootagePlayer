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
#   GET /clip/<id>/index.m3u8  the clip's stream: rendering starts at the first request, from the start (?t=<s> elsewhere)
#   GET /clip/<id>/<file>      its init segment and segments
#
# Environment: AFP_LIBRARY (folders, ':'-separated; default /clips), AFP_CACHE (/stream/cache), AFP_BIN (/stream: afp-render,
# gyroflow-gpu-table, the shaders), AFP_PORT (8099), AFP_HEIGHT (1080), AFP_BITRATE (12M)
import hashlib, html, json, os, re, shutil, subprocess, threading, time, urllib.parse
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler

LIBRARY = [p for p in os.environ.get("AFP_LIBRARY", "/clips").split(":") if p]
CACHE = os.environ.get("AFP_CACHE", "/stream/cache")
BIN = os.environ.get("AFP_BIN", "/stream")
PORT = int(os.environ.get("AFP_PORT", "8099"))
HEIGHT = int(os.environ.get("AFP_HEIGHT", "1080"))
BITRATE = os.environ.get("AFP_BITRATE", "12M")
MAX_JOBS = 2
VIDEO = re.compile(r"\.(mp4|mov)$", re.I)

jobs = {}  # key -> {proc, dir, started}
lock = threading.Lock()


def clips():
    out = []
    for root in LIBRARY:
        for d, _, files in os.walk(root):
            if "/_" in d or "/." in d:
                continue
            for f in sorted(files):
                if VIDEO.search(f) and not f.startswith("."):
                    path = os.path.join(d, f)
                    out.append({"id": hashlib.sha1(path.encode()).hexdigest()[:12], "path": path, "name": os.path.relpath(path, root)})
    return out


def probe(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=width,height,pix_fmt,color_transfer:format=duration", "-of", "json", path], capture_output=True, text=True)
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


def start_job(clip, start):
    key = f"{clip['id']}-{int(start)}"
    with lock:
        j = jobs.get(key)
        if j and (j["proc"].poll() is None or os.path.exists(os.path.join(j["dir"], "index.m3u8"))):
            return j
        # At most MAX_JOBS renders at once: the oldest running one stops
        running = sorted((v for v in jobs.values() if v["proc"].poll() is None), key=lambda v: v["started"])
        while len(running) >= MAX_JOBS:
            old = running.pop(0)
            old["proc"].terminate()
        out = os.path.join(CACHE, key)
        shutil.rmtree(out, ignore_errors=True)
        os.makedirs(out, exist_ok=True)
        s, _ = probe(clip["path"])
        w, h = int(s.get("width") or 3840), int(s.get("height") or 2160)
        args = [os.path.join(BIN, "afp-render")]
        shaders = []
        # The warp: the clip's project, else its own motion data with the underwater preset (the player's default.gyroflow)
        project = project_of(clip["path"])
        ud = {"project": project or clip["path"], "input_width": w, "input_height": h, "interpolation": "bicubic",
              "gpu_chromatic": True, "edges": "smooth", "edges_blur": 2, "underwater": "project" if project else "auto",
              "level": "medium", "lookahead": 0}
        if not project:
            ud["preset"] = os.path.join(BIN, "underwater.gyroflow")
        shader = os.path.join(out, "warp.glsl")
        r = subprocess.run([os.path.join(BIN, "gyroflow-gpu-table"), "--player", json.dumps(ud), shader], capture_output=True, text=True)
        reply = {}
        try:
            reply = json.loads(r.stdout.strip().splitlines()[-1])
        except Exception:
            reply = {"error": (r.stderr or r.stdout)[-300:]}
        ow, oh = w, h
        if reply.get("shader"):
            shaders.append(shader)
            ow, oh = reply["output"]
        else:
            log("no warp for", clip["name"], reply)
        height = min(HEIGHT, oh)
        width = int(round(height * ow / oh / 2)) * 2
        # DJI's D-Log M (10-bit, untagged): its look to HLG, as the player's hdr look
        if os.path.basename(clip["path"]).startswith("DJI_") and "10" in (s.get("pix_fmt") or "") and s.get("color_transfer") in (None, "", "unknown", "bt709"):
            shaders.append(os.path.join(BIN, "dlogm_hlg.glsl"))
            args.append("--hlg")
        args += ["-w", str(width), "-h", str(height), "-b", BITRATE, "-ss", str(start), clip["path"], out] + shaders
        logf = open(os.path.join(out, "render.log"), "w")
        env = dict(os.environ, LD_LIBRARY_PATH="/opt/pl/lib")
        proc = subprocess.Popen(args, stdout=logf, stderr=subprocess.STDOUT, env=env)
        log("rendering", clip["name"], "from", start, "s:", reply.get("mode", "no warp"), " ".join(args[1:]))
        j = {"proc": proc, "dir": out, "started": time.time()}
        jobs[key] = j
        return j


def ready(j, segments=2, timeout=45):
    t = time.time()
    playlist = os.path.join(j["dir"], "index.m3u8")
    while time.time() - t < timeout:
        if os.path.exists(playlist):
            with open(playlist) as f:
                text = f.read()
            if text.count("#EXTINF") >= segments or "#EXT-X-ENDLIST" in text:
                return text
        if j["proc"].poll() not in (None, 0):
            return None
        time.sleep(0.25)
    return None


class Handler(BaseHTTPRequestHandler):
    def send(self, code, body, ctype, extra=None):
        data = body if isinstance(body, bytes) else body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
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
        q = urllib.parse.parse_qs(u.query)
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
            return self.send(200, f"<!doctype html><meta charset=utf-8><body style='margin:0;background:#000'>"
                                  f"<video src='/clip/{parts[1]}/index.m3u8' controls autoplay style='width:100vw;height:100vh'></video>", "text/html")
        if len(parts) == 3 and parts[0] == "clip":
            clip = next((c for c in clips() if c["id"] == parts[1]), None)
            if not clip:
                return self.send(404, "no such clip", "text/plain")
            start = float((q.get("t") or ["0"])[0])
            key = f"{clip['id']}-{int(start)}"
            if parts[2] == "index.m3u8":
                j = start_job(clip, start)
                text = ready(j)
                if text is None:
                    return self.send(500, "the render failed: see render.log in " + j["dir"], "text/plain")
                return self.send(200, text, "application/vnd.apple.mpegurl")
            j = jobs.get(key) or {"dir": os.path.join(CACHE, key)}
            path = os.path.join(j["dir"], os.path.basename(parts[2]))
            if not os.path.exists(path):
                return self.send(404, "not yet", "text/plain")
            with open(path, "rb") as f:
                data = f.read()
            return self.send(200, data, "video/mp4" if path.endswith(".mp4") else "video/iso.segment" if path.endswith(".m4s") else "text/plain")
        return self.send(404, "not found", "text/plain")

    def log_message(self, fmt, *args):
        pass


if __name__ == "__main__":
    os.makedirs(CACHE, exist_ok=True)
    log(f"afp-stream on :{PORT}, library {LIBRARY}, {len(clips())} clips")
    ThreadingHTTPServer(("", PORT), Handler).serve_forever()
