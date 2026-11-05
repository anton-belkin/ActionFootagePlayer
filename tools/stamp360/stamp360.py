#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The 360 stamp's test (docs/360-cameras.md, "The stamp"; step 1 of docs/360-implementation-plan.md): does the stamp
vs-gyroflow writes into the luma plane reach a gpu-next user shader unchanged?

    stamp360.py clips   [--out DIR]   synthetic clips: 8- and 10-bit, limited and full range
    stamp360.py encode  [--out DIR]   headless: each clip through the app's mpv and the plugin's Stamp360Test, decoded in
                                      software and with videotoolbox-copy, written losslessly; the stamp read back from
                                      the files and compared with what the plugin logged. Also the stamped clips the
                                      screen test plays without VapourSynth (lossless HEVC)
    stamp360.py screen  [--out DIR]   ON SCREEN (mpv windows, ~3 min): each case played in gpu-next with
                                      shaders/stamp_probe.glsl, a screenshot of each of the first frames, the probe's
                                      decoded values read from them and compared with the log
    stamp360.py check   [--out DIR]   the comparison of `screen` again, from its screenshots

The cases on screen: the clips through VapourSynth (as the player has them) decoded in software and with
videotoolbox-copy, uploaded as they come out of VapourSynth (planar: yuv420p, yuv420p10) and converted to the
decoder's semi-planar format (nv12, p010) first; and the stamped lossless clips with no VapourSynth, decoded both ways
(VideoToolbox's p010 and nv12 straight to the renderer).

Needs the app's own mpv (tools/dev_app.sh; Homebrew's can't load VSScript) and the plugin built with
`cargo build --release` in vs-gyroflow/. gpu-next hangs while the Mac is locked: run `screen` with the display awake.
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
PLUGIN = REPO / 'vs-gyroflow/target/release/libvs_gyroflow.dylib'
PROBE = REPO / 'shaders/stamp_probe.glsl'
APP = Path(os.environ.get('AFP_APP', Path.home() / 'Applications/ActionFootagePlayer.app'))
MPV = APP / 'Contents/Helpers/mpv'
FFMPEG = APP / 'Contents/Helpers/ffmpeg'

W, H, FPS, FRAMES = 1920, 1080, 30, 12
SHOTS = 6

# As vs-gyroflow/src/stamp.rs
CODES, HEADER, BLOCK, ROWS = 32, 7, 2, 2
MAGIC = [12, 3, 9, 6]
VERSION = 1

# name: (depth, range, encoder arguments)
CLIPS = {
    '8bit-limited':  (8, 'tv', ['-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-color_range', 'tv', '-crf', '18']),
    '8bit-full':     (8, 'pc', ['-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-color_range', 'pc', '-crf', '18']),
    '10bit-limited': (10, 'tv', ['-c:v', 'libx265', '-pix_fmt', 'yuv420p10le', '-color_range', 'tv', '-tag:v', 'hvc1',
                                 '-x265-params', 'log-level=error:range=limited']),
    '10bit-full':    (10, 'pc', ['-c:v', 'libx265', '-pix_fmt', 'yuv420p10le', '-color_range', 'pc', '-tag:v', 'hvc1',
                                 '-x265-params', 'log-level=error:range=full']),
}
HWDECS = ['no', 'videotoolbox-copy']


def env():
    """The app's runtime, as macos/player.sh sets it up"""
    e = dict(os.environ)
    contents = APP / 'Contents'
    homes = sorted((contents / 'Frameworks/Python.framework/Versions').glob('[0-9]*'))
    if homes:
        e['PYTHONHOME'] = str(homes[-1])
        for vs in homes[-1].glob('lib/python*/site-packages/vapoursynth/libvsscript.dylib'):
            e['VSSCRIPT_PATH'] = str(vs)
    e['PATH'] = f"{contents / 'Resources/bin'}:{contents / 'Helpers'}:{e['PATH']}"
    icd = contents / 'Resources/vulkan/icd.d/MoltenVK_icd.json'
    e['VK_DRIVER_FILES'] = e['VK_ICD_FILENAMES'] = str(icd)
    return e


def quoted(s):
    """An mpv option value taken as it is (mpv's %length% quoting)"""
    return f'%{len(s.encode())}%{s}'


def vf_stamp(log):
    return f"vapoursynth=file={quoted(str(HERE / 'stamp.vpy'))}:user-data={quoted(f'{PLUGIN}|{log}')}"


# ---------------------------------------------------------------------------------------------------
# The stamp, as stamp.rs writes and reads it

def mask(bits):
    return (1 << bits) - 1


def fine_code(level, bits, depth):
    s = depth - bits
    return (level << s) | ((1 << (s - 1)) if s > 0 else 0)


def expected_codes(e, depth):
    """The codes stamp.rs writes for a logged frame"""
    b = e['bits']
    codes = [fine_code(level, 4, depth) for level in MAGIC + [depth - 1, b - 1, VERSION]]
    levels = [(e['time'] >> (b * (3 - i))) & mask(b) for i in range(4)]
    for q in e['q']:
        levels += [q >> b, q & mask(b)]
    for y, c in zip(e['yaw'], e['confidence']):
        levels += [y >> b, y & mask(b), c]
    levels.append(sum(levels) & mask(b))
    return codes + [fine_code(level, b, depth) for level in levels]


def decode_codes(codes, depth):
    """stamp.rs's Fixed::from_codes, as a dict like the log's; or the reason it isn't one"""
    header = [c >> (depth - 4) for c in codes[:HEADER]]
    if header[:4] != MAGIC:
        return 'no magic'
    if header[4] + 1 != depth:
        return f'written at {header[4] + 1} bits'
    if header[6] != VERSION:
        return f'version {header[6]}'
    b = header[5] + 1
    levels = [c >> (depth - b) for c in codes[HEADER:]]
    if sum(levels[:-1]) & mask(b) != levels[-1]:
        return 'check'
    f = {'bits': b, 'time': 0, 'q': [], 'yaw': [], 'confidence': []}
    for i in range(4):
        f['time'] = (f['time'] << b) | levels[i]
    for c in range(4):
        f['q'].append((levels[4 + 2 * c] << b) | levels[5 + 2 * c])
    for h in range(4):
        i = 12 + 3 * h
        f['yaw'].append((levels[i] << b) | levels[i + 1])
        f['confidence'].append(levels[i + 2])
    return f


FIELDS = ('bits', 'time', 'q', 'yaw', 'confidence')


def load_log(path):
    by_time = {}
    for line in Path(path).read_text().splitlines():
        e = json.loads(line)
        by_time[e['time']] = e
    return by_time


def compare(got, log, depth):
    """None when `got` (decoded) is a frame of the log exactly, else what differs"""
    if isinstance(got, str):
        return got
    e = log.get(got['time'])
    if e is None:
        return f"time {got['time']:#x} isn't in the log"
    diff = [k for k in FIELDS if got[k] != e[k]]
    if e['depth'] != depth:
        diff.append(f"depth {e['depth']} written")
    return ', '.join(diff) or None


# ---------------------------------------------------------------------------------------------------

def run(cmd, **kw):
    r = subprocess.run([str(c) for c in cmd], capture_output=True, text=True, env=env(), **kw)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, cmd))}\n{r.stdout[-3000:]}\n{r.stderr[-3000:]}")
    return r


def clips(out):
    out.mkdir(parents=True, exist_ok=True)
    for name, (depth, rng, codec) in CLIPS.items():
        path = out / f'src-{name}.mp4'
        if path.exists():
            continue
        # A moving picture in grey: neutral chroma, so the probe's black and white bits stay black and white
        src = f'testsrc2=s={W}x{H}:r={FPS}:d={FRAMES / FPS},format=gray'
        run([FFMPEG, '-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', src, *codec, path])
        print('made', path.name)


def read_raw_frames(path, depth):
    """The luma of each frame of a file, decoded in software, as lists of codes of the stamp's rows"""
    # The planes as they are: to gray, ffmpeg would stretch a limited range
    fmt = 'yuv420p' if depth == 8 else 'yuv420p10le'
    r = subprocess.run([FFMPEG, '-hide_banner', '-loglevel', 'error', '-i', str(path), '-f', 'rawvideo', '-pix_fmt', fmt, '-'],
                       capture_output=True, env=env(), check=True)
    size = W * H * 3 // 2 * (1 if depth == 8 else 2)
    frames = []
    for k in range(len(r.stdout) // size):
        data = memoryview(r.stdout)[k * size:(k + 1) * size]
        def at(x, y):
            i = y * W + x
            return data[i] if depth == 8 else data[2 * i] | (data[2 * i + 1] << 8)
        codes, alike = [], True
        for i in range(CODES):
            block = [at(i * BLOCK + dx, H - ROWS + dy) for dy in range(ROWS) for dx in range(BLOCK)]
            codes.append(block[0])
            alike &= len(set(block)) == 1
        frames.append((codes, alike))
    return frames


def encode(out):
    clips(out)
    ok = True
    for name, (depth, rng, _) in CLIPS.items():
        for hwdec in HWDECS:
            case = out / f'encode-{name}-{hwdec}'
            case.mkdir(exist_ok=True)
            log = case / 'stamp.log'
            log.unlink(missing_ok=True)
            dst = case / 'stamped.nut'
            run([MPV, '--no-config', '--load-scripts=no', f'--hwdec={hwdec}', f'--vf={vf_stamp(log)}', out / f'src-{name}.mp4',
                 f'--o={dst}', '--of=nut', '--ovc=ffv1', '--oac=', '--no-audio'])
            frames = read_raw_frames(dst, depth)
            entries = load_log(log)
            bad = [(k, compare(decode_codes(c, depth), entries, depth) or ('' if alike else 'blocks differ'))
                   for k, (c, alike) in enumerate(frames)]
            bad = [(k, why) for k, why in bad if why]
            ok &= not bad and len(frames) > 0
            print(f"{name:14} {hwdec:18} {len(frames) - len(bad)}/{len(frames)} frames exact" + (f'  {bad[:3]}' if bad else ''))
        # The stamped clip the screen test plays without VapourSynth: lossless HEVC, which VideoToolbox may decode
        stamped = out / f'stamped-{name}.mp4'
        log = out / f'stamped-{name}.log'
        if not stamped.exists():
            log.unlink(missing_ok=True)
            pix = 'yuv420p' if depth == 8 else 'yuv420p10le'
            run([MPV, '--no-config', '--load-scripts=no', '--hwdec=no', f'--vf={vf_stamp(log)}', out / f'src-{name}.mp4',
                 f'--o={stamped}', '--ovc=libx265', f'--ovcopts=x265-params=lossless=1:log-level=error,tag=hvc1',
                 f'--vf-append=format=fmt={pix}', '--no-audio'])
        frames = read_raw_frames(stamped, depth)
        bad = [k for k, (c, alike) in enumerate(frames) if compare(decode_codes(c, depth), load_log(log), depth) or not alike]
        ok &= not bad
        print(f"{name:14} {'lossless HEVC':18} {len(frames) - len(bad)}/{len(frames)} frames exact")
    print('encode:', 'all exact' if ok else 'NOT ALL EXACT')
    return ok


def screen_cases(out):
    cases = []
    for name, (depth, rng, _) in CLIPS.items():
        semi = 'nv12' if depth == 8 else 'p010'
        for hwdec in HWDECS:
            for upload in ('planar', semi):
                case = f'vs-{name}-{hwdec}-{upload}'
                cases.append((case, depth, out / f'src-{name}.mp4', hwdec, upload, out / f'screen-{case}/stamp.log'))
            cases.append((f'file-{name}-{hwdec}', depth, out / f'stamped-{name}.mp4', hwdec, None, out / f'stamped-{name}.log'))
    return cases


def screen(out):
    if not (out / 'stamped-10bit-full.mp4').exists():
        encode(out)
    for case, depth, src, hwdec, upload, log in screen_cases(out):
        through_vs = upload is not None
        d = out / f'screen-{case}'
        d.mkdir(exist_ok=True)
        for f in d.glob('*'):
            f.unlink()
        args = [MPV, '--no-config', '--load-scripts=no', '--vo=gpu-next', f'--hwdec={hwdec}', '--pause', '--keep-open=yes',
                '--autocreate-playlist=no', '--osc=no', '--osd-level=0', '--autofit=40%', '--no-audio',
                f'--glsl-shaders={PROBE}', f'--script={HERE / "collect.lua"}',
                f'--script-opts=stamp360-out={d},stamp360-frames={SHOTS}', '--screenshot-format=png', src]
        if through_vs:
            args.append(f'--vf={vf_stamp(log)}')
            if upload != 'planar':
                args.append(f'--vf-append=format=fmt={upload}')
        r = subprocess.run([str(a) for a in args], capture_output=True, text=True, env=env(), timeout=120)
        (d / 'mpv.log').write_text(r.stdout + r.stderr)
        print(f'{case:42} played ({r.returncode})')
    return check(out)


def read_shot(path, depth):
    """The probe's grid from a screenshot: the words it painted"""
    r = subprocess.run([FFMPEG, '-hide_banner', '-loglevel', 'error', '-i', str(path), '-f', 'rawvideo', '-pix_fmt', 'gray', '-'],
                       capture_output=True, env=env(), check=True)
    data = r.stdout
    w = int(subprocess.run([FFMPEG.parent / 'ffprobe', '-v', 'error', '-select_streams', 'v', '-show_entries', 'stream=width',
                            '-of', 'csv=p=0', str(path)], capture_output=True, text=True, env=env()).stdout.strip())
    h = len(data) // w
    sx, sy = w / W, h / H
    words = []
    for row in range(65):
        word = 0
        for bit in range(32):
            x, y = int((32 + 8 * bit + 4) * sx), int((32 + 8 * row + 4) * sy)
            word = (word << 1) | (1 if data[y * w + x] > 127 else 0)
        words.append(word)
    return words


def check(out):
    ok, rows = True, []
    for case, depth, src, hwdec, upload, log_path in screen_cases(out):
        d = out / f'screen-{case}'
        props = json.loads((d / 'props.json').read_text()) if (d / 'props.json').exists() else []
        log = load_log(log_path) if log_path.exists() else {}
        good, notes = 0, []
        for p in props:
            words = read_shot(p['shot'], depth)
            if words[0] != 0xF0F0F0F0:
                notes.append('no probe in the screenshot')
                continue
            status = words[1]
            got = {'bits': (status >> 8) & 0xFF, 'time': (words[2] << 32) | words[3], 'q': words[4:8], 'yaw': words[8:12],
                   'confidence': words[12:16]}
            codes = []
            for wd in words[17:33]:
                codes += [wd >> 16, wd & 0xFFFF]
            why = None
            if not status >> 31 & 1:
                why = f'no magic; codes {codes[:8]}'
            elif not status >> 30 & 1:
                why = 'check'
            elif not status >> 29 & 1:
                why = 'blocks differ'
            elif status & 0xFF != depth:
                why = f'depth {status & 0xFF}'
            else:
                why = compare(got, log, depth)
            if why and not why.startswith('no magic'):
                e = log.get(got['time'])
                if e:
                    want = expected_codes(e, depth)
                    why += f'; codes off by {[g - x for g, x in zip(codes, want)]}'
            if why:
                notes.append(why)
            else:
                good += 1
        fmt = lambda k: (props[0].get(k) or {}).get('hw-pixelformat') or (props[0].get(k) or {}).get('pixelformat') if props else '?'
        info = f"dec {fmt('decoded')}, hwdec {props[0]['hwdec'] if props else '?'}, vo gets {fmt('filtered')}" \
               f" {(props[0].get('filtered') or {}).get('colorlevels', '') if props else ''}"
        exact = props and good == len(props)
        ok &= bool(exact)
        rows.append((case, good, len(props), info, notes[:2]))
        print(f"{case:42} {good}/{len(props)} exact   {info}" + (f'   {notes[:2]}' if notes else ''))
    (out / 'results.json').write_text(json.dumps(rows, indent=1))
    print('screen:', 'all exact' if ok else 'NOT ALL EXACT')
    return ok


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('what', choices=['clips', 'encode', 'screen', 'check'])
    ap.add_argument('--out', type=Path, default=Path(os.environ.get('TMPDIR', '/tmp')) / 'stamp360')
    a = ap.parse_args()
    if not MPV.exists():
        sys.exit(f'no app mpv at {MPV} (AFP_APP=the app; tools/dev_app.sh builds one)')
    if not PLUGIN.exists():
        sys.exit(f'no plugin at {PLUGIN}: cargo build --release in vs-gyroflow/')
    a.out.mkdir(parents=True, exist_ok=True)
    ok = {'clips': lambda o: clips(o) or True, 'encode': encode, 'screen': screen, 'check': check}[a.what](a.out)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
