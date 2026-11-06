# mpv vf=vapoursynth script for a 360 camera's own file, set up by main.lua (the gyroflow plugin's 360 mode): the lens
# tracks mpv joined side by side (lavfi-complex hstack) into the equirect sphere in the camera's axes, with the stamp
# under it (vs-gyroflow/src/sphere360.rs, docs/360-cameras.md). user_data (JSON): plugin, video (the camera's file, for
# its rig and motion), workers, cache_mb, and optionally interpolation (bilinear, bicubic, lanczos), width (the sphere's), distance (the stitching distance, m),
# time_offset (a moment played as a video of its own). The plugin keeps the clip loaded between mpv's reloads (seeks).
import json
import vapoursynth as vs

core = vs.core
args = json.loads(user_data)  # noqa: F821 (set by mpv)

# VapourSynth's frame cache is 8 GB by default: ~90 frames of 8K, which took mpv to 5-7 GB on a 16 GB Mac and slowed
# 8K30 with the kernel's memory work (docs/360-throughput-spike.md)
core.max_cache_size = int(args.get('cache_mb', 1500))

if not hasattr(core, 'gyroflow'):
    core.std.LoadPlugin(args['plugin'])

opts = dict(video=args['video'], workers=int(args.get('workers', 3)))
if args.get('width'):
    opts['width'] = int(args['width'])
if args.get('distance'):
    opts['distance'] = float(args['distance'])
if args.get('interpolation'):
    opts['interpolation'] = str(args['interpolation'])
if args.get('time_offset'):
    opts['time_offset'] = float(args['time_offset'])

core.gyroflow.Sphere360(video_in, **opts).set_output()  # noqa: F821 (set by mpv)
