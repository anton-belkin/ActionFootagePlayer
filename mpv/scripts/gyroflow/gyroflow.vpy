# mpv vf=vapoursynth script, set up by main.lua (the gyroflow plugin).
#
# Stabilizes mpv's video_in with the Gyroflow project given in user_data (JSON: plugin, project,
# preset, interpolation, workers, fast, wide, horizon_lock, underwater, width, height, edges*, chromatic). The project can be the video itself (its own motion data). mpv reloads this script on every seek; the plugin keeps the loaded project and
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
if args.get('horizon_lock') is not None:
    stab['horizon_lock'] = float(args['horizon_lock'])
if args.get('underwater'):
    stab['underwater'] = args['underwater']
# The size to render at, for the display (main.lua); the project's or the input's without it
if args.get('width') and args.get('height'):
    stab['width'] = int(args['width'])
    stab['height'] = int(args['height'])
# What fills the uncovered edges; the control file has the mode and the blur as they change while playing
options = {}
for key, kind in (('edges', str), ('edges_blur', float), ('edges_zoom', float), ('edges_control', str)):
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

core.gyroflow.Stabilize(clip, **stab, **options).set_output()
