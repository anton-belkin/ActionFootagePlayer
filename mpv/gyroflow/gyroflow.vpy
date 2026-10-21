# mpv vf=vapoursynth script, set up by scripts/gyroflow_auto.lua.
#
# Stabilizes mpv's video_in with the Gyroflow project given in user_data (JSON: plugin, project,
# preset, interpolation, workers). The project can be the video itself (embedded project, or its own motion data). mpv reloads this script on every seek; the plugin keeps the loaded project and
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

core.gyroflow.Stabilize(
    video_in,  # noqa: F821 (set by mpv)
    project=args['project'],
    interpolation=args.get('interpolation', 'bicubic'),
    workers=args.get('workers', 2),
    **options,
).set_output()
