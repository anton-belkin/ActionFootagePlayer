#!/usr/bin/env python3
# The GPU path's table kept ahead of where a clip plays (docs/gpu-table-ahead.md), end to end and headless: a clip
# "plays" in real time (a clock, and optionally the video's own reads paced at its bitrate) against gyroflow-gpu-table
# --serve, the table asked for as the player does it: ahead.lua's own decisions, through luajit, each quarter second.
# It fails if any moment played without motion data in the table (it must wait there instead).
#
#   tests/gpu_ahead_e2e.py <gyroflow-gpu-table> <clip> <out prefix> [--seconds 120] [--video] [--throttle MBps]
#                          [--catalog <project>] [--walk <dir>...] [--old] [--seek t:pos,...] [--loop a:b] [--cold]
#
#   --throttle   the server's motion reads held to that many MB/s (AFP_READ_MBPS, prefetch.rs): a slow share
#   --video      the clip's own reads meanwhile, 1 MB at a time, 2 s ahead of where it plays (as mpv): their times
#   --catalog    afp-catalog derived, footage and list on that project every 10 s (read-only; its cache in the
#                scratch HOME): their times
#   --walk       the side panel's old clip count meanwhile: every entry under these folders stat'ed, again and again
#   --old        the protocol before: the stretch, then the whole clip asked for at once ("full")
#   --cold       the clip taken out of this Mac's file cache first (msync MS_INVALIDATE; the NAS's own cache stays)
#
# The shaders and the caches go to a scratch HOME next to <out prefix>; the server's log to <out prefix>.server.log.
# Example (2026-11-19, the 1.7 GB Osmo Action 6 clip on the NAS, cold, with its video and the catalog):
#   tests/gpu_ahead_e2e.py vs-gyroflow/target/release/gyroflow-gpu-table /Volumes/photo/.../DJI_..._0103_D.MP4 /tmp/run \
#       --cold --video --catalog "/Volumes/ssd/Action Footage Library"
# Without a NAS, a slow share's speed: --throttle 8 on any clip with its own motion data.
import json, os, subprocess, sys, threading, time, argparse
ap = argparse.ArgumentParser()
ap.add_argument('table'); ap.add_argument('clip'); ap.add_argument('out')
ap.add_argument('--video', action='store_true'); ap.add_argument('--catalog'); ap.add_argument('--walk', nargs='*')
ap.add_argument('--old', action='store_true'); ap.add_argument('--play-at', type=float, default=0.0)
ap.add_argument('--seconds', type=float, default=120); ap.add_argument('--cold', action='store_true'); ap.add_argument('--throttle', type=float)
ap.add_argument('--loop', default='')
ap.add_argument('--seek', default='')   # "t:pos,...": at play position t, a seek to pos
a = ap.parse_args()
here = os.path.dirname(os.path.abspath(__file__))
AHEAD_DIR = os.path.join(here, '..', 'mpv', 'scripts', 'gyroflow')
home = os.path.abspath(a.out) + '.home'
os.makedirs(home, exist_ok=True)
os.environ['HOME'] = home
def evict(path):
    # The file's pages out of this Mac's cache: mapped, msync(MS_INVALIDATE), unmapped
    import ctypes, mmap
    libc = ctypes.CDLL(None, use_errno=True)
    libc.mmap.restype = ctypes.c_void_p
    libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
    libc.msync.argtypes = libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t] + [ctypes.c_int]
    libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    fd, n = os.open(path, os.O_RDONLY), os.path.getsize(path)
    p = libc.mmap(None, n, mmap.PROT_READ, mmap.MAP_SHARED, fd, 0)
    libc.msync(p, n, 2 if sys.platform == 'darwin' else 2)   # MS_INVALIDATE
    libc.munmap(p, n); os.close(fd)
if a.cold: evict(a.clip)
sock = f'/tmp/afp-rig-{os.getpid()}.sock'
senv = dict(os.environ, AFP_GPU_SERVER_LOG=a.out + '.server.log')
if a.throttle: senv['AFP_READ_MBPS'] = str(a.throttle)
srv = subprocess.Popen([a.table, '--serve', sock], env=senv)
time.sleep(0.3)
size = os.path.getsize(a.clip)
dur = float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', a.clip], capture_output=True, text=True).stdout)
rate = size / dur
base = {"project": a.clip, "level": "medium", "input_width": 3840, "input_height": 2160, "edges": "smooth", "edges_blur": 2}
n_ask = [0]
def ask(extra):
    n_ask[0] += 1
    t = time.time()
    r = subprocess.run([a.table, '--player', json.dumps(dict(base, **extra)), f'{a.out}.{n_ask[0]}.glsl', '--server', sock], capture_output=True, text=True)
    try: rep = json.loads(r.stdout.splitlines()[0])
    except Exception: rep = {'error': r.stderr[-300:]}
    try: os.remove(f'{a.out}.{n_ask[0]}.glsl')
    except OSError: pass
    return time.time() - t, rep
stop = threading.Event()
# ahead.lua's decision for one state: "start until" or "- -", then "wait" or "go", then "urgent" or "-"
DECIDE = '''package.path = arg[1] .. "/?.lua;" .. package.path
local ahead = require 'ahead'
local s = loadstring("return " .. arg[2])()
local n = ahead.next(s)
if n then s.busy = true end
print(n and string.format("%.3f %.3f", n.start, n.until_) or "- -", ahead.wait(s) and "wait" or "go", n and n.urgent and "urgent" or "-")'''
# The play clock: advances while playing and not held
clock = {'pos': 0.0, 'held': False, 'playing': False, 'last': None, 'held_s': 0.0, 'unstab_s': 0.0, 'unsettled_s': 0.0, 'played': 0.0}
lock = threading.Lock()
def tick_clock():
    now = time.time()
    with lock:
        if clock['last'] is not None and clock['playing']:
            if clock['held']: clock['held_s'] += now - clock['last']
            else: clock['pos'] = min(dur, clock['pos'] + now - clock['last']); clock['played'] += now - clock['last']
        clock['last'] = now
        return clock['pos']
lat, lat_at = [], []
def video():
    f = open(a.clip, 'rb', buffering=0)
    while not stop.is_set():
        pos = tick_clock()
        want = min(size, int((pos + 2) * rate))
        off = f.tell()
        if want < off - 4 * rate: f.seek(int(pos * rate)); continue   # a seek back
        if want > off + 8 * rate: f.seek(int(pos * rate)); off = f.tell()
        while off < want and not stop.is_set():
            t = time.time(); f.read(1 << 20); lat.append(time.time() - t); lat_at.append((time.time() - t, t - t0, pos)); off += 1 << 20
        time.sleep(0.05)
cat_times = []
def catalog():
    exe = os.path.join(os.path.dirname(a.table), 'afp-catalog')
    env = dict(os.environ, AFP_HOME=home + '/afp')
    while not stop.is_set():
        for c in (['derived', a.catalog, '--clip', 'x'], ['footage', a.catalog], ['list', a.catalog, '--kind', 'moment']):
            t = time.time(); subprocess.run([exe] + c, capture_output=True, env=env); cat_times.append((c[0], time.time() - t))
        stop.wait(10)
t0 = time.time()
ms, r = ask({"start": 0})
t_first = time.time() - t0
state = {'covers': r.get('covers'), 'settled': r.get('settled'), 'busy': False}
dur = r.get('duration') or dur   # the server's (by frames), as main.lua has it
if a.old and r.get('partial'): state['covers'], state['settled'] = [0.0, 20.0], 20.0
print(f'first table {t_first:.1f} s covers {state["covers"]}', flush=True)
time.sleep(a.play_at)
with lock: clock['playing'] = True; clock['last'] = time.time()
if a.video: threading.Thread(target=video, daemon=True).start()
if a.catalog: threading.Thread(target=catalog, daemon=True).start()
def walk():
    # The side panel's clip count before (moments.footage): every entry listed and stat'ed, depth 8, again and again
    def one(d, depth):
        try: names = os.listdir(d)
        except OSError: return
        subs = []
        for name in names:
            if stop.is_set(): return
            if name.startswith('.'): continue
            try: st = os.stat(os.path.join(d, name))
            except OSError: continue
            if (st.st_mode & 0o170000) == 0o040000: subs.append(name)
        if depth < 8:
            for x in subs: one(os.path.join(d, x), depth + 1)
    while not stop.is_set():
        for r in a.walk: one(r, 0)
if a.walk: threading.Thread(target=walk, daemon=True).start()
walker = None
steps = []
def extend(start, until, urgent):
    t = time.time()
    _, rep = ask({"start": start, "until": until, "urgent": urgent})
    with lock:
        if rep.get('shader'):
            state['covers'] = rep.get('covers'); state['settled'] = rep.get('settled')
        state['busy'] = False
    steps.append((start, until, time.time() - t, rep.get('error')))
def old_full():
    t = time.time()
    _, rep = ask({"full": True})
    with lock: state['covers'] = None if rep.get('shader') else state['covers']; state['busy'] = False
    steps.append((0, dur, time.time() - t, rep.get('error')))
if a.old:
    state['busy'] = True
    threading.Thread(target=old_full, daemon=True).start()
seeks = [tuple(map(float, x.split(':'))) for x in a.seek.split(',') if x]
loop = tuple(map(float, a.loop.split(':'))) if a.loop else None
margins, settled_margins = [], []
last_print = 0
while True:
    pos = tick_clock()
    if clock['played'] >= a.seconds or pos >= dur - 0.1: break
    if seeks and pos >= seeks[0][0]:
        with lock: clock['pos'] = seeks[0][1]
        pos = seeks.pop(0)[1]
    with lock:
        c, st, busy = state['covers'], state['settled'], state['busy']
    if loop and pos >= loop[1]:
        with lock: clock['pos'] = loop[0]
        pos = loop[0]
    if not a.old and c:
        lua = '{pos=%f,duration=%f,paused=false,covers={%f,%f},settled=%f,busy=%s%s}' % (pos, dur, c[0], c[1], st, 'true' if busy else 'false',
                                                                                   ',loop={a=%f,b=%f}' % loop if loop else '')
        out = subprocess.run(['luajit', '-e', DECIDE, '/dev/null', AHEAD_DIR, lua], capture_output=True, text=True).stdout.split()
        if out and out[0] != '-' and not busy:
            with lock: state['busy'] = True
            threading.Thread(target=extend, args=(float(out[0]), float(out[1]), out[3] == 'urgent'), daemon=True).start()
        with lock: clock['held'] = bool(out) and out[2] == 'wait'
    covered = c[1] if c else dur
    stab = c is None or (pos <= c[1] and pos >= (c[0] + 4 if c[0] > 0 else 0))
    if not stab and not clock['held']: clock['unstab_s'] += 0.25
    # Past what's settled (not the clip's end): frames the next table changes, a jump as it comes in
    elif c and st is not None and c[1] < dur and pos > st + 0.3 and not clock['held']: clock['unsettled_s'] += 0.25
    margins.append(covered - pos)
    if c: settled_margins.append(st - pos)
    if time.time() - last_print >= 5:
        last_print = time.time()
        print(f'  t={time.time()-t0:5.1f}s pos {pos:5.1f}s covers {c} margin {covered-pos:+5.1f}s{" HELD" if clock["held"] else ""}{" UNSTABILIZED" if not stab and not clock["held"] else ""}', flush=True)
    time.sleep(0.25)
stop.set()
if walker: walker.kill()
print(f'SUMMARY first-table {t_first:.1f}s; margin min {min(margins):+.1f}s p10 {sorted(margins)[len(margins)//10]:+.1f}s; '
      f'held {clock["held_s"]:.1f}s; unstabilized {clock["unstab_s"]:.1f}s; past settled {clock["unsettled_s"]:.1f}s over {a.seconds:.0f}s of playback')
for s in steps: print(f'  step {s[0]:.1f}->{s[1]:.1f} s in {s[2]:.1f} s {s[3] or ""}')
if lat_at: print('  slowest video reads (s, at s, play pos s): ' + ', '.join(f'{d:.2f}@{w:.0f}/{p:.0f}' for d, w, p in sorted(lat_at, reverse=True)[:5]))
if lat: l = sorted(lat); print(f'  video 1 MB reads: p50 {l[len(l)//2]*1000:.0f} ms p95 {l[int(len(l)*.95)]*1000:.0f} ms max {l[-1]*1000:.0f} ms (n={len(l)})')
if cat_times:
    for k in ('derived', 'footage', 'list'):
        v = sorted(t for n, t in cat_times if n == k)
        if v: print(f'  afp-catalog {k}: median {v[len(v)//2]:.2f} s max {v[-1]:.2f} s (n={len(v)})')
srv.terminate()
sys.exit(1 if (clock['unstab_s'] > 0 or clock['unsettled_s'] > 0) and not a.old else 0)
