# mpv vf=vapoursynth script, set up by main.lua (the gyroflow plugin).
#
# Stabilizes mpv's video_in with the Gyroflow project given in user_data (JSON: plugin, project,
# preset, interpolation, workers, fast, fov, edges*). The project can be the video itself (embedded project, or its own motion data). mpv reloads this script on every seek; the plugin keeps the loaded project and
# its GPU setup between reloads, so that's cheap.
import json
import vapoursynth as vs

core = vs.core
args = json.loads(user_data)  # noqa: F821 (set by mpv)

if not hasattr(core, 'gyroflow'):
    core.std.LoadPlugin(args['plugin'])

# The stabilization: the same for LeavesEdges and Stabilize, so they share one loaded project. fov: the wider view
# (Ctrl+g), in place of the project's FOV
stab = dict(project=args['project'], interpolation=args.get('interpolation', 'bicubic'), workers=args.get('workers', 2))
if args.get('preset'):
    stab['preset'] = args['preset']
if args.get('fov') is not None:
    stab['fov'] = float(args['fov'])
# What fills the uncovered edges; the control file has the mode and the blur as they change while playing
options = {}
for key, kind in (('edges', str), ('edges_blur', float), ('edges_zoom', float), ('edges_control', str)):
    if args.get(key) is not None:
        options[key] = kind(args[key])

clip = video_in  # noqa: F821 (set by mpv)
if args.get('fast') and options.get('edges', 'off') != 'off':
    # A clip faster than max_fps whose stabilized frame leaves edges somewhere (Gyroflow's measurement, see
    # LeavesEdges): filling them, the sphere trail above all, doesn't keep up with every frame. So every other one,
    # before the stabilization: the skipped ones cost nothing but decoding. The durations double with it, which is
    # what mpv times the output by
    edges = core.gyroflow.LeavesEdges(clip, **stab)
    if (edges['uncovered'] if isinstance(edges, dict) else edges) > 0:
        clip = core.std.SelectEvery(clip, cycle=2, offsets=0)

core.gyroflow.Stabilize(clip, **stab, **options).set_output()
