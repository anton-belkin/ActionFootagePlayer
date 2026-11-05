# mpv vf=vapoursynth script, set up by main.lua (the gyroflow plugin).
#
# Stabilizes mpv's video_in with the Gyroflow project given in user_data (JSON: plugin, project,
# preset, interpolation, workers, fast, wide, level, lookahead, horizon_lock, underwater, rotation, aspect, projection, width, height, edges*, chromatic, still_control). The project can be the video itself (its own motion data). mpv reloads this script on every seek; the plugin keeps the loaded project and
# its GPU setup between reloads, so that's cheap.
import json
import vapoursynth as vs

core = vs.core
args = json.loads(user_data)  # noqa: F821 (set by mpv)

if not hasattr(core, 'gyroflow'):
    core.std.LoadPlugin(args['plugin'])

# The stabilization: the same for LeavesEdges and Stabilize, so they share one loaded project. wide: the wider view
# (Ctrl+g), no zoom at the FOV the plugin finds for the clip; horizon_lock: the lock's amount (%), in place of the
# project's; underwater: yes, no, or auto (as the camera says, vs-gyroflow/src/water.rs)
stab = dict(project=args['project'], interpolation=args.get('interpolation', 'bicubic'), workers=args.get('workers', 2))
if args.get('preset'):
    stab['preset'] = args['preset']
if args.get('wide'):
    stab['wide'] = 1
# A clip without a project: its level (vs-gyroflow/src/levels.rs; the control file switches it), and how much of its
# motion data it waits for (motion.rs)
if args.get('level'):
    stab['level'] = args['level']
if args.get('lookahead') is not None:
    stab['lookahead'] = float(args['lookahead'])
# A moment played as a video of its own: its frames' times from 0, its part starting this far into the clip
if args.get('time_offset'):
    stab['time_offset'] = float(args['time_offset'])
# A moment without a project of its own, stabilized on its own: its part, seconds of the clip
# A moment's switch as it's viewed: its project's "only within the trim range", whatever the project says
if args.get('alone') is not None:
    stab['alone'] = 1 if args['alone'] else 0
if args.get('trim_start') is not None and args.get('trim_end') is not None:
    stab['trim_start'] = float(args['trim_start'])
    stab['trim_end'] = float(args['trim_end'])
if args.get('horizon_lock') is not None:
    stab['horizon_lock'] = float(args['horizon_lock'])
if args.get('underwater'):
    stab['underwater'] = args['underwater']
# The output frame (the side panel's): turned clockwise by 0, 90, 180 or 270 degrees, then cut to an aspect ratio
# (W:H, or original), in place of the project's own output size and rotation (vs-gyroflow/src/lib.rs, Frame)
if args.get('rotation') is not None:
    stab['rotation'] = int(args['rotation'])
if args.get('aspect'):
    stab['aspect'] = str(args['aspect'])
# Gyroflow's output projection (0 rectilinear, 1 stereographic, 2 equidistant, 3 equisolid, 4 orthographic), in place
# of the project's or preset's
if args.get('projection') is not None:
    stab['projection'] = int(args['projection'])
# The size to render at, for the display (main.lua); the project's or the input's without it
if args.get('width') and args.get('height'):
    stab['width'] = int(args['width'])
    stab['height'] = int(args['height'])
# What fills the uncovered edges; the control file has the mode and the blur as they change while playing
options = {}
for key, kind in (('edges', str), ('edges_blur', float), ('edges_control', str)):
    if args.get(key) is not None:
        options[key] = kind(args[key])
# Each frame's lens geometry in its last row, for chromatic.glsl (vs-gyroflow/src/dispersion.rs), which paints it over
if args.get('chromatic'):
    options['chromatic'] = 1

clip = video_in  # noqa: F821 (set by mpv)
if args.get('fast') and options.get('edges', 'off') != 'off':
    # A clip faster than max_fps (main.lua; off unless set) whose stabilized frame leaves edges somewhere (Gyroflow's
    # measurement, see LeavesEdges), on a computer where filling them doesn't keep up with every frame. So every other one,
    # before the stabilization: the skipped ones cost nothing but decoding. The durations double with it, which is
    # what mpv times the output by
    edges = core.gyroflow.LeavesEdges(clip, **stab)
    if (edges['uncovered'] if isinstance(edges, dict) else edges) > 0:
        clip = core.std.SelectEvery(clip, cycle=2, offsets=0)

# The paused frame cleaned of its noise (still.lua): while mpv seeks to it again, the control file says which frame. The
# frames before its neighbours aren't stabilized (mpv drops them), and Still rebuilds it from them; the others pass.
# Without a time it's a seek to the frame on screen with the chain set up anew (paused at full size, and playing on from
# it): only the frames before it are left out
still = None
if args.get('still_control'):
    try:
        with open(args['still_control']) as f:
            still = json.load(f)
    except (OSError, ValueError):
        still = None
if still:
    options['skip_before'] = float(still['skip_before'])
elif args.get('still_control'):
    # The GPU for it set up a few seconds into playback, so the first pause doesn't wait for that (once per process)
    options['still_warm_up'] = 5.0

out = core.gyroflow.Stabilize(clip, **stab, **options)
if still and still.get('time') is not None:
    out = core.gyroflow.Still(out, time=float(still['time']), radius=int(still['radius']), luma_keep=float(still.get('luma_keep', 0.25)))
out.set_output()
