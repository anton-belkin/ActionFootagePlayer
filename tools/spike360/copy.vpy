# tools/spike360/copy.vpy: what stands in for vs-gyroflow's 360 mode in the throughput spike: mpv's video_in (both
# lenses side by side, from lavfi-complex hstack) copied into a new frame with VapourSynth's stock functions, no
# plugin. user_data (vf=vapoursynth's user-data, JSON) says how:
#
#   out   "same": a frame of the input's size (std.FlipVertical: one plain copy, so the picture is upside down)
#         "8k":   a frame of the sphere's size, 8192×4096 (std.AddBorders around the input: one copy and the
#                 borders filled), as the plugin's equirect will be
#         "none": the input as it is, no copy (VapourSynth's frame handed back)
#   cache VapourSynth's frame cache in MB (core.max_cache_size; default 8192 at R81, ~90 frames of 8K)
#   sync  a file: each frame's number read from both lenses' barcodes (tools/spike360/make_clips.sh), appended as
#         "<n> <left> <right>", to tell whether the tracks stay in step. Costs a little: not for the perf runs
import json
import vapoursynth as vs

core = vs.core
args = json.loads(user_data) if user_data else {}  # noqa: F821 (set by mpv)
clip = video_in  # noqa: F821 (set by mpv)
if args.get("cache"):
    core.max_cache_size = int(args["cache"])

if args.get("sync"):
    log = open(args["sync"], "a", buffering=1)
    lens = clip.width // 2

    def strip(x):
        # The 32 blocks' middle rows, one sample per block (each the mean of its 64 columns)
        return core.resize.Bilinear(clip.std.CropAbs(width=2048, height=32, left=x, top=272), 32, 1,
                                    format=vs.GRAY16)

    def number(f):
        row = f[0]
        return sum(1 << i for i in range(32) if row[0, i] > 32768)

    def read(n, f):
        log.write(f"{n} {number(f[1])} {number(f[2])}\n")
        return f[0]

    clip = core.std.ModifyFrame(clip, [clip, strip(256), strip(lens + 256)], read)

mode = args.get("out", "same")
if mode == "8k":
    w, h = 8192, 4096
    left, top = (w - clip.width) // 2 & ~1, (h - clip.height) // 2 & ~1
    clip = core.std.AddBorders(clip, left, w - clip.width - left, top, h - clip.height - top)
elif mode == "same":
    clip = core.std.FlipVertical(clip)

clip.set_output()
