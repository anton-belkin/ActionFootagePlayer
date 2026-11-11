#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Action Footage NAS Helper: what a NAS does for the player and the Library, in one container (tools/nas-helper/
# README.md). It runs, as its settings say (nas_settings.py; `helper.py --help` lists them):
#
#   the guardian   afp-catalog watch, one per project: the project's catalog kept current from the NAS's side
#   the stream     afp_stream.py: each clip, moment and clip preview as HLS, as the player plays it
#   the links      jellyfin.py: the stream's moments and clip previews as a Jellyfin library (its scope)
#
# Each is a process of its own, started again when it stops (after a pause that grows while it keeps failing); their
# output goes to the container's log, each line with its part's name. What each is doing is in /data/helper.json,
# which the stream's /settings page shows.
#
#   helper.py            run
#   helper.py --help     the settings and folders, described
#   helper.py --markdown the same as Markdown tables (README.md)
#   helper.py --check    what would run, and the settings' problems, then exit

import json, os, signal, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import nas_settings as S  # noqa: E402

def _version():
    try:
        with open(os.path.join(HERE, "VERSION")) as f:
            return f.read().strip()
    except OSError:
        return "dev"


VERSION = _version()
BIN = os.environ.get("AFP_BIN", "/opt/afp/bin")
PLAYER = os.environ.get("AFP_PLAYER", "/opt/afp/player")
DATA, CONFIG = "/data", "/config"
STATUS = os.path.join(DATA, "helper.json")
PROJECT_FILE = "Project.afproject"

status = {"version": VERSION, "started": time.strftime("%Y-%m-%dT%H:%M:%S"), "parts": {}}
status_lock = threading.Lock()
children = []
stopping = threading.Event()


def log(*a):
    print(time.strftime("%H:%M:%S"), "helper:", *a, flush=True)


def save_status(name=None, **kw):
    with status_lock:
        if name:
            status["parts"].setdefault(name, {}).update(kw)
        os.makedirs(DATA, exist_ok=True)
        tmp = STATUS + ".tmp"
        with open(tmp, "w") as f:
            json.dump(status, f, ensure_ascii=False, indent=1)
        os.replace(tmp, STATUS)


def projects():
    """The projects (AFP_PROJECTS, else every folder in /projects with a Project.afproject)"""
    given = [p for p in S.get("AFP_PROJECTS").split(":") if p]
    if given:
        return given
    try:
        names = sorted(os.listdir("/projects"))
    except OSError:
        return []
    return [os.path.join("/projects", n) for n in names if os.path.isfile(os.path.join("/projects", n, PROJECT_FILE))]


def location_hosts(projs):
    """The hosts the projects' network locations name (//HOST/share/...)"""
    hosts = []
    for p in projs:
        try:
            with open(os.path.join(p, PROJECT_FILE)) as f:
                doc = json.load(f)
        except (OSError, ValueError):
            continue
        for loc in doc.get("locations") or []:
            path = (loc.get("path") or "").replace("\\", "/") if isinstance(loc, dict) else ""
            if path.startswith("//") and len(path.split("/")) > 3:
                h = path.split("/")[2]
                if h and h not in hosts:
                    hosts.append(h)
    return hosts


def shares(projs):
    """[(//HOST/share, dir)]: AFP_SHARES, else each folder in /shares as a share of the host the locations name"""
    given = S.get("AFP_SHARES")
    if given:
        out = []
        for part in given.split(";"):
            src, eq, d = part.strip().partition("=")
            if eq and src.startswith("//"):
                out.append((src.rstrip("/"), d))
        return out
    try:
        names = sorted(n for n in os.listdir("/shares") if os.path.isdir(os.path.join("/shares", n)))
    except OSError:
        return []
    hosts = location_hosts(projs) or ["NAS"]
    return [(f"//{h}/{n}", os.path.join("/shares", n)) for h in hosts for n in names]


def read_only(path):
    try:
        return bool(os.statvfs(path).f_flag & os.ST_RDONLY)
    except OSError:
        return False


class Part(threading.Thread):
    """A process kept running: started again when it stops, after 5 s, doubling up to 10 min while it fails within a
    minute of starting. `stop_codes`: exit codes that mean starting again won't help (it waits an hour then)"""

    def __init__(self, name, args, env=None, cwd=None, user=None, stop_codes=(), umask=None):
        super().__init__(daemon=True, name=name)
        self.args, self.env, self.cwd, self.user, self.stop_codes, self.umask = args, env, cwd, user, stop_codes, umask
        self.proc = None
        save_status(name, state="starting", command=" ".join(args), restarts=0)

    def preexec(self):
        os.setsid()
        if self.umask is not None:
            os.umask(self.umask)
        if self.user:
            uid, _, gid = self.user.partition(":")
            if gid:
                os.setgid(int(gid))
            os.setuid(int(uid))

    def run(self):
        pause, restarts = 5, 0
        while not stopping.is_set():
            t = time.time()
            try:
                self.proc = subprocess.Popen(self.args, env=dict(os.environ, **(self.env or {})), cwd=self.cwd, stdout=subprocess.PIPE,
                                             stderr=subprocess.STDOUT, preexec_fn=self.preexec, text=True, errors="replace", bufsize=1)
            except OSError as e:
                save_status(self.name, state="failed", last=str(e))
                log(self.name, "couldn't start:", e)
                stopping.wait(600)
                continue
            children.append(self.proc)
            save_status(self.name, state="running", pid=self.proc.pid, since=time.strftime("%Y-%m-%dT%H:%M:%S"))
            last = ""
            for line in self.proc.stdout:
                line = line.rstrip("\n")
                if line:
                    last = line
                    print(f"{self.name}: {line}", flush=True)
            code = self.proc.wait()
            children.remove(self.proc)
            if stopping.is_set():
                break
            restarts += 1
            if code in self.stop_codes:
                wait = 3600
            else:
                pause = 5 if time.time() - t > 60 else min(pause * 2, 600)
                wait = pause
            save_status(self.name, state="stopped", exit=code, last=last[-300:], restarts=restarts,
                        again=time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(time.time() + wait)))
            log(self.name, f"stopped (exit {code}): starting again in {wait} s")
            stopping.wait(wait)


def plan():
    """What runs: [(name, Part arguments)], and the problems that keep a part from running"""
    projs = projects()
    sh = shares(projs)
    parts, problems = [], list(S.problems())
    if not projs:
        problems.append("no project: bind a project's folder to /projects/<its name> (a folder with Project.afproject), or set AFP_PROJECTS")
    if S.on("AFP_GUARDIAN"):
        for p in projs:
            if read_only(p):
                problems.append(f"the guardian needs {p} read-write (it writes the catalog in .afp/catalog)")
                continue
            args = [os.path.join(BIN, "afp-catalog"), "watch", p]
            for src, d in sh:
                name = src.rstrip("/").split("/")[-1]
                if f"{name}={d}" not in args:
                    args += ["--share", f"{name}={d}"]
            # As root (UGOS refuses inotify on root's folders to other users), what it writes open to all; exit 3: the
            # project is newer than this helper
            parts.append((f"guardian[{os.path.basename(p)}]", dict(args=args, env={"AFP_HOME": os.path.join(DATA, "catalog"), "COMPUTERNAME": S.get("AFP_COMPUTER_NAME")},
                                                              umask=0, stop_codes=(3,))))
    if S.on("AFP_STREAM"):
        # The stream reads the projects read-only: /library/<name> for /projects/<name>
        lib = []
        for p in projs:
            view = os.path.join("/library", os.path.basename(p)) if p.startswith("/projects/") else p
            if not os.path.isdir(view):
                problems.append(f"the stream needs {view}: the project bound again, read-only ({p} is the guardian's)")
            elif not read_only(view):
                problems.append(f"the stream refuses {view}: it must be bound read-only (':ro'), so that the player it asks can't write in the project")
            else:
                lib.append(view)
        if lib:
            stream = os.path.join(DATA, "stream")
            env = {k: S.get(k) for k in ("AFP_PORT", "AFP_HEIGHT", "AFP_BITRATE", "AFP_SEGMENT", "AFP_CACHE_GB", "AFP_OPENING",
                                         "AFP_INTERP", "AFP_CHROMATIC", "AFP_TV_LOOK", "AFP_LOG_REQUESTS")}
            env.update(AFP_LIBRARY=":".join(lib), AFP_SHARES=";".join(f"{s}={d}" for s, d in sh), AFP_BIN=BIN, AFP_PLAYER=PLAYER,
                       AFP_PLAYER_CONFIG=os.path.join(CONFIG, "player"), AFP_CACHE=os.path.join(stream, "cache"),
                       AFP_ARGS=os.path.join(stream, "args"), AFP_GENERATED=os.path.join(stream, "shaders"), AFP_HELPER_STATUS=STATUS)
            parts.append(("stream", dict(args=[sys.executable, "-u", os.path.join(HERE, "afp_stream.py")], env=env)))
    if S.on("AFP_JELLYFIN") and S.on("AFP_STREAM") and S.get("AFP_PUBLIC_URL") and S.get("AFP_JELLYFIN_SCOPE") in ("moments", "clips", "all"):
        if not os.path.isdir("/jellyfin"):
            problems.append("the links need /jellyfin: the folder Jellyfin's library reads, bound in read-write")
        else:
            port = S.get("AFP_PORT")
            args = [sys.executable, "-u", os.path.join(HERE, "jellyfin.py"), "--catalog", f"http://127.0.0.1:{port}/catalog.json",
                    "--stream", S.get("AFP_PUBLIC_URL"), "--stream-local", f"http://127.0.0.1:{port}", "--out", "/jellyfin",
                    "--scope", S.get("AFP_JELLYFIN_SCOPE"), "--names", S.get("AFP_JELLYFIN_NAMES"), "--every", S.get("AFP_JELLYFIN_EVERY")]
            if os.path.exists(S.get("AFP_JELLYFIN_KEY_FILE")):
                args += ["--jellyfin", S.get("AFP_JELLYFIN_URL"), "--key-file", S.get("AFP_JELLYFIN_KEY_FILE")]
                if S.get("AFP_JELLYFIN_PATH"):
                    args += ["--jellyfin-path", S.get("AFP_JELLYFIN_PATH")]
            else:
                problems.append(f"no Jellyfin API key in {S.get('AFP_JELLYFIN_KEY_FILE')}: the library isn't refreshed after a change")
            parts.append(("jellyfin", dict(args=args, user=S.get("AFP_JELLYFIN_USER") or None)))
    return projs, sh, parts, problems


def main():
    if "--help" in sys.argv or "-h" in sys.argv:
        print(f"Action Footage NAS Helper {VERSION}\n\nSettings: the container's environment. Folders: its volumes.")
        print(S.text())
        return
    if "--markdown" in sys.argv:
        print(S.markdown(), end="")
        return
    projs, sh, parts, problems = plan()
    log(f"Action Footage NAS Helper {VERSION}")
    log("projects:", ", ".join(projs) or "none")
    log("shares:", ", ".join(f"{s} = {d}" for s, d in sh) or "none")
    for p in problems:
        log("problem:", p)
    log("runs:", ", ".join(n for n, _ in parts) or "nothing")
    if "--check" in sys.argv:
        for n, kw in parts:
            print(n, " ".join(kw["args"]))
        sys.exit(1 if problems else 0)
    status.update(projects=projs, shares=[f"{s}={d}" for s, d in sh], problems=problems,
                  settings={s[0]: S.get(s[0]) for s in S.SETTINGS})
    status["parts"] = {}
    save_status()
    # The player's defaults: a folder of your own (/config/player/script-opts), empty at first
    os.makedirs(os.path.join(CONFIG, "player", "script-opts"), exist_ok=True)

    def stop(*_):
        stopping.set()
        for p in list(children):
            try:
                os.killpg(p.pid, signal.SIGTERM)
            except OSError:
                pass

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    threads = [Part(n, **kw) for n, kw in parts]
    for t in threads:
        t.start()
    while not stopping.wait(1):
        pass
    for t in threads:
        t.join(timeout=10)
    log("stopped")


if __name__ == "__main__":
    main()
