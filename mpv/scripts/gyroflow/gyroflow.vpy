# mpv vf=vapoursynth script, set up by main.lua (the gyroflow plugin).
#
# Stabilizes mpv's video_in with the Gyroflow project given in user_data (JSON: plugin, project,
# preset, interpolation, workers, half_rate, edges*). The project can be the video itself (embedded project, or its own motion data). mpv reloads this script on every seek; the plugin keeps the loaded project and
# its GPU setup between reloads, so that's cheap.
import json
import vapoursynth as vs

core = vs.core
args = json.loads(user_data)  # noqa: F821 (set by mpv)

if not hasattr(core, 'gyroflow'):
    core.std.LoadPlugin(args['plugin'])

options = {}
if args.get('preset'):
    options['preset'] = args['preset']
# What fills the uncovered edges; the control file has the mode as it changes while playing
for key, kind in (('edges', str), ('edges_blur', float), ('edges_zoom', float), ('edges_control', str)):
    if args.get(key) is not None:
        options[key] = kind(args[key])

clip = video_in  # noqa: F821 (set by mpv)
if args.get('half_rate'):
    # Every other frame, before the stabilization: the skipped ones cost nothing but decoding. The durations
    # double with it, which is what mpv times the output by
    clip = core.std.SelectEvery(clip, cycle=2, offsets=0)

core.gyroflow.Stabilize(
    clip,
    project=args['project'],
    interpolation=args.get('interpolation', 'bicubic'),
    workers=args.get('workers', 2),
    **options,
).set_output()
