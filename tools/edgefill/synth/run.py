#!/usr/bin/env python3
# The synthetic bench: each scenario through the real shader (afpchain, offscreen) and through model.py, frame by
# frame on the same frames, then side by side.
#   run.py --template <kept shader> --afpchain <afpchain> --out <dir> [--size 1280x720] [--sw 2048] [scenario ...]
# [--set KEY=VALUE ...] [--no-shader: the default model where the shader was, to try a rule in the model first]
# Per scenario in <dir>/<scenario>/: compare.mp4 (top: shader | model; bottom: the world as it is | the difference x4),
# sheet.jpg (four moments of the same), metrics.csv, and the scenario's line in <dir>/summary.txt.
# The template: a shader gyroflow-export kept (AFP_GPU_KEEP=1, --edges sphere --wide) for any clip; its table is
# replaced with the scenario's cameras and its look dropped (synth.make_shader).
import argparse, os, subprocess, sys, tempfile
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synth as S
from model import Model


def luma(x):
    return x @ np.array([0.2126, 0.7152, 0.0722])


def to8(x):
    return (np.clip(x, 0, 1) * 255 + 0.5).astype(np.uint8)


def half(x):
    return 0.25 * (x[0::2, 0::2] + x[1::2, 0::2] + x[0::2, 1::2] + x[1::2, 1::2])


def run(name, args):
    W, H = args.W, args.H
    d = os.path.join(args.out, name)
    os.makedirs(d, exist_ok=True)
    p = S.path(name, args.frames)
    cams = S.cameras(p)
    shader = os.path.join(d, 'shader.glsl')
    S.make_shader(args.template, cams, shader)
    fifo = os.path.join(tempfile.mkdtemp(), 'raw')
    os.mkfifo(fifo)
    env = dict(os.environ, AFP_RAW_OUT=fifo)
    proc = None if args.no_shader else subprocess.Popen([args.afpchain, str(W), str(H), str(S.FPS), '0', str(W), str(H), '-', '-',
                             'gyroflow_time_offset=0', '--', shader], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, env=env)
    model = Model(W, H, **args.params)
    base = Model(W, H, **args.base) if args.no_shader else None
    enc = subprocess.Popen(['ffmpeg', '-v', 'error', '-y', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{W}x{H}', '-r', str(S.FPS),
                            '-i', '-', '-c:v', 'libx264', '-crf', '18', '-preset', 'fast', '-pix_fmt', 'yuv420p',
                            os.path.join(d, 'compare.mp4')], stdin=subprocess.PIPE)
    rows, prev_s, prev_m, sheet = [], None, None, []
    keys = {int(len(p) * f) for f in (0.25, 0.5, 0.75)} | {len(p) - 1}
    raw = None
    for n, ((qv, qc), v) in enumerate(zip(cams, p)):
        which, gain = v[4], v[5]
        frame = S.render_frame(W, H, qc, which, gain)
        if proc:
            proc.stdin.write(S.to_yuv420p10(frame))
            proc.stdin.flush()
            if raw is None:
                raw = open(fifo, 'rb')
            buf = raw.read(W * H * 8)
            sh = np.frombuffer(buf, dtype='<u2').reshape(H, W, 4)[..., :3] / 65535.0
        else:
            # no shader: the model with the default rule in its place (an idea against the current one)
            sh = base.step(frame, qv, qc)
        mo = model.step(frame, qv, qc)
        tr = model.truth(qv, which, gain)
        fill = model.m < 1          # past the frame and its feather
        diff = np.abs(luma(sh) - luma(mo))
        r = dict(frame=n, fill_px=int(fill.sum()),
                 shader_vs_model=float(diff[fill].mean()) if fill.any() else 0.0,
                 shader_vs_world=float(np.abs(luma(sh) - luma(tr))[fill].mean()) if fill.any() else 0.0,
                 model_vs_world=float(np.abs(luma(mo) - luma(tr))[fill].mean()) if fill.any() else 0.0,
                 shader_flicker=float(np.abs(luma(sh) - luma(prev_s))[fill].mean()) if prev_s is not None and fill.any() else 0.0,
                 model_flicker=float(np.abs(luma(mo) - luma(prev_m))[fill].mean()) if prev_m is not None and fill.any() else 0.0)
        rows.append(r)
        prev_s, prev_m = sh, mo
        dv = np.repeat(np.clip(np.abs(sh - mo).max(-1, keepdims=True) * 4, 0, 1), 3, -1)
        grid = np.concatenate([np.concatenate([half(sh), half(mo)], 1), np.concatenate([half(tr), half(dv)], 1)], 0)
        enc.stdin.write(to8(grid).tobytes())
        if n in keys:
            sheet.append(grid)
    if proc:
        proc.stdin.close()
        proc.wait()
    enc.stdin.close()
    enc.wait()
    from PIL import Image
    Image.fromarray(to8(np.concatenate(sheet, 0))).save(os.path.join(d, 'sheet.jpg'), quality=88)
    with open(os.path.join(d, 'metrics.csv'), 'w') as f:
        f.write(','.join(rows[0].keys()) + '\n')
        for r in rows:
            f.write(','.join(f'{v:.5f}' if isinstance(v, float) else str(v) for v in r.values()) + '\n')
    m = lambda k: np.mean([r[k] for r in rows[1:]])
    line = (f'{name:16s} shader-model {m("shader_vs_model"):.4f}  shader-world {m("shader_vs_world"):.4f}  '
            f'model-world {m("model_vs_world"):.4f}  flicker shader {m("shader_flicker"):.4f} model {m("model_flicker"):.4f}')
    print(line, flush=True)
    with open(os.path.join(args.out, 'summary.txt'), 'a') as f:
        f.write(line + '\n')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--template', required=True)
    ap.add_argument('--afpchain', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--size', default='1280x720')
    ap.add_argument('--set', action='append', default=[], help="a model parameter, KEY=VALUE (model.DEFAULTS)")
    ap.add_argument('--base', action='append', default=[], help="with --no-shader: a parameter of the model in the shader's place")
    ap.add_argument('--no-shader', action='store_true', help="the default model in the shader's place")
    ap.add_argument('--frames', type=int, default=0)
    ap.add_argument('scenarios', nargs='*')
    args = ap.parse_args()
    args.W, args.H = map(int, args.size.split('x'))
    args.frames = args.frames or None
    from model import DEFAULTS
    args.params = {}
    for kv in args.set:
        k, v = kv.split('=', 1)
        args.params[k] = type(DEFAULTS[k])(v)
    base, args.base = args.base, {}
    for kv in base:
        k, v = kv.split('=', 1)
        args.base[k] = type(DEFAULTS[k])(v)
    for name in args.scenarios or list(S.SCENARIOS):
        run(name, args)


if __name__ == '__main__':
    main()
