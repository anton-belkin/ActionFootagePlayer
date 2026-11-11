# SPDX-License-Identifier: GPL-3.0-or-later
#
# The NAS Helper's settings: one list, read by the helper (helper.py: what runs, and how), shown by `helper.py --help`
# and on the stream's page /settings, and copied into tools/nas-helper/README.md. Each is an environment variable of
# the container (compose.yaml's `environment:`).

import os

# The folders inside the container, bound to the NAS's folders by the container's volumes
FOLDERS = [
    ("/projects/<name>", "read-write", "Each project (the folder with Project.afproject), under its own folder's name: the guardian keeps its "
     "catalog in <project>/.afp/catalog. Nothing else is written there."),
    ("/library/<name>", "read-only", "The same project folders again, read-only, for the stream: the player it asks may want to fill a missing "
     "key into a moment's link, and here that write fails, as it should. The stream refuses a folder it could write to."),
    ("/shares/<share>", "read-only", "The network shares the projects' locations are on (//NAS/photo/... in Project.afproject), each "
     "under its share's name, as a Mac finds them mounted."),
    ("/data", "read-write", "The helper's own: the guardian's computer name and caches (/data/catalog), the stream's renders "
     "(/data/stream/cache, up to AFP_CACHE_GB) and what the player said (/data/stream/args)."),
    ("/config", "read-write", "What you set: the player's defaults (/config/player/script-opts/gyroflow.conf and dlogm.conf, as in "
     "the app's mpv/script-opts), Jellyfin's API key (/config/jellyfin.key)."),
    ("/jellyfin", "read-write", "Only with AFP_JELLYFIN=on: the folder a Jellyfin library reads (a Home Videos library). The "
     "helper writes only there and removes what it wrote."),
]

# name, default, part, what it does; choices where there are some
SETTINGS = [
    # What runs
    ("AFP_GUARDIAN", "on", "parts", "The library guardian (afp-catalog watch): keeps each project's catalog current from "
     "the NAS's side. The system tells it what changes on the NAS's own disks, so what any computer (or the NAS itself) "
     "changes reaches the other computers' players and Libraries in about a second, without them walking the share. A "
     "project newer than this helper is refused, not touched: update the helper.", ("on", "off")),
    ("AFP_STREAM", "on", "parts", "The stream (afp-stream): every clip, moment and clip preview as an HLS stream, as the "
     "player plays it (stabilized on the GPU, D-Log M to HDR, the project's settings), rendered by the NAS's GPU as it "
     "is watched. Its page: http://<nas>:AFP_PORT/. Needs /dev/dri (an Intel GPU with VA-API).", ("on", "off")),
    ("AFP_JELLYFIN", "off", "parts", "Links for Jellyfin: a folder (/jellyfin) a Jellyfin Home Videos library reads, a "
     ".strm (the stream's URL), .nfo and cover per item, sorted into trips; the library refreshed after a change. Needs "
     "the stream.", ("on", "off")),

    # What the parts look at
    ("AFP_PROJECTS", "", "projects", "The projects, ':'-separated paths inside the container. Empty: every folder in "
     "/projects that has a Project.afproject."),
    ("AFP_SHARES", "", "projects", "Where the shares the projects' locations name are here: "
     "'//HOST/share=/shares/share;...'. Empty: every folder in /shares, as a share of the NAS the projects' locations "
     "name (//DISKSTATIONDXP/photo for /shares/photo)."),

    # The stream
    ("AFP_PORT", "8099", "stream", "The stream's port (its page, the streams, /settings)."),
    ("AFP_HEIGHT", "1080", "stream", "The streams' height in pixels (never more than the clip's)."),
    ("AFP_BITRATE", "30M", "stream", "The streams' bitrate (HEVC). It costs no speed; a TV's 100 Mbit/s port is the limit."),
    ("AFP_SEGMENT", "1", "stream", "Seconds per HLS segment."),
    ("AFP_CACHE_GB", "40", "stream", "What played stays in /data/stream/cache up to this many GB; the clips used longest ago "
     "go first. A clip's renders go when its project or these settings change."),
    ("AFP_OPENING", "8", "stream", "Seconds of each clip that 'pre-render openings' renders ahead, so a folder played in a "
     "row starts each clip at once."),
    ("AFP_INTERP", "bilinear", "stream", "The warp's interpolation. bilinear is the fastest (67-69 fps at 1080p on an "
     "N100); bspline, bicubic and lanczos are sharper and slower.", ("bilinear", "bspline", "bicubic", "lanczos")),
    ("AFP_CHROMATIC", "0", "stream", "1: the water's color fringes taken out where the player does it (slower: ~48 fps on "
     "an N100, not real time at 4K60).", ("0", "1")),
    ("AFP_TV_LOOK", "hlg", "stream", "What TVs that take only MPEG-TS HLS get (Samsung Tizen before 5, LG webOS before "
     "3.5; also any stream asked with ?fmt=ts): hlg, pq (HDR10) or sdr, optionally followed by x<peak> (highlights "
     "expanded up to that many nits), g<gain> (the whole picture brighter) and r<fps> (at most that many frames a "
     "second), e.g. pqg2.5r30."),
    ("AFP_LOG_REQUESTS", "1", "stream", "1: every request in the log, with its User-Agent (who asked for what); 0: none.", ("0", "1")),

    # Jellyfin
    ("AFP_JELLYFIN_SCOPE", "all", "jellyfin", "What goes to Jellyfin: moments (each moment of a project, in its trip), "
     "clips (each clip preview: the whole clip) or all. What's outside it is removed from /jellyfin.",
     ("moments", "clips", "all")),
    ("AFP_PUBLIC_URL", "", "jellyfin", "The stream as Jellyfin's players reach it, written into each .strm: "
     "http://<the NAS's address>:AFP_PORT. Required with AFP_JELLYFIN=on."),
    ("AFP_JELLYFIN_URL", "http://127.0.0.1:8096", "jellyfin", "Jellyfin's own address, to refresh the library after a "
     "change (with the container on the host's network, 127.0.0.1 and Jellyfin's port)."),
    ("AFP_JELLYFIN_KEY_FILE", "/config/jellyfin.key", "jellyfin", "A file with a Jellyfin API key (Dashboard → API Keys). "
     "Without it the library isn't refreshed; Jellyfin finds the changes at its next scan."),
    ("AFP_JELLYFIN_PATH", "", "jellyfin", "The /jellyfin folder as Jellyfin's library has it (its own container's path, "
     "e.g. /data/_Action Footage), to refresh only that library. Empty: /jellyfin."),
    ("AFP_JELLYFIN_NAMES", "en", "jellyfin", "The language of the folders and descriptions it writes (Moments/Clips, "
     "No trip...).", ("en", "ru")),
    ("AFP_JELLYFIN_EVERY", "120", "jellyfin", "Seconds between looks at the stream's catalog."),
    ("AFP_JELLYFIN_USER", "", "jellyfin", "uid:gid the files in /jellyfin are written as (e.g. 1000:10). Empty: root."),
]

DEFAULTS = {s[0]: s[1] for s in SETTINGS}


def get(name):
    """A setting's value: the environment's, else its default"""
    v = os.environ.get(name)
    return v if v not in (None, "") else DEFAULTS.get(name, "")


def on(name):
    return get(name).strip().lower() in ("on", "1", "yes", "true")


def problems():
    """Settings with a value outside their choices, or missing where they're required"""
    out = []
    for s in SETTINGS:
        if len(s) > 4 and get(s[0]) and get(s[0]) not in s[4]:
            out.append(f"{s[0]}={get(s[0])!r}: one of {', '.join(s[4])}")
    if on("AFP_JELLYFIN") and not get("AFP_PUBLIC_URL"):
        out.append("AFP_PUBLIC_URL is required with AFP_JELLYFIN=on (http://<the NAS's address>:" + get("AFP_PORT") + ")")
    if on("AFP_JELLYFIN") and not on("AFP_STREAM"):
        out.append("AFP_JELLYFIN=on needs AFP_STREAM=on: the links are the stream's")
    return out


PARTS = [("parts", "What runs"), ("projects", "What it looks at"), ("stream", "The stream"), ("jellyfin", "Links for Jellyfin")]


def text():
    """The settings as plain text (helper.py --help)"""
    lines = []
    for part, title in PARTS:
        lines += ["", title.upper(), ""]
        for s in SETTINGS:
            if s[2] != part:
                continue
            cur = os.environ.get(s[0])
            head = f"  {s[0]}  (default {s[1]!r}" + (f"; one of {', '.join(s[4])}" if len(s) > 4 else "") + ")"
            if cur not in (None, "") and cur != s[1]:
                head += f"  now {cur!r}"
            lines.append(head)
            lines += wrap(s[3], "      ")
            lines.append("")
    lines += ["FOLDERS (the container's volumes)", ""]
    for path, mode, what in FOLDERS:
        lines.append(f"  {path}  ({mode})")
        lines += wrap(what, "      ")
        lines.append("")
    return "\n".join(lines)


def markdown():
    """The settings as a Markdown table per part (README.md)"""
    out = []
    for part, title in PARTS:
        out += [f"### {title}", "", "| Setting | Default | What it does |", "|---|---|---|"]
        for s in SETTINGS:
            if s[2] == part:
                choices = f" One of: {', '.join('`%s`' % c for c in s[4])}." if len(s) > 4 else ""
                out.append(f"| `{s[0]}` | `{s[1]}` | {s[3].replace('|', '/')}{choices} |" if s[1] else f"| `{s[0]}` | | {s[3]}{choices} |")
        out.append("")
    out += ["### Folders", "", "| In the container | | What it is |", "|---|---|---|"]
    out += [f"| `{p}` | {m} | {w} |" for p, m, w in FOLDERS]
    return "\n".join(out) + "\n"


def wrap(s, indent, width=110):
    words, lines, line = s.split(), [], indent
    for w in words:
        if len(line) + len(w) + 1 > width and line.strip():
            lines.append(line.rstrip())
            line = indent
        line += w + " "
    if line.strip():
        lines.append(line.rstrip())
    return lines
