# mpv vf=vapoursynth script for the 360 stamp's test (stamp360.py): stamps each frame with the plugin's test values
# (core.gyroflow.Stamp360Test) and logs what it wrote. user_data: the plugin's path, a "|", the log's path
import vapoursynth as vs

core = vs.core
plugin, log = user_data.split('|', 1)  # noqa: F821 (set by mpv)
if not hasattr(core, 'gyroflow'):
    core.std.LoadPlugin(plugin)
if not hasattr(core.gyroflow, 'Stamp360Test'):
    raise vs.Error('the gyroflow plugin loaded has no Stamp360Test: another one was loaded first')
core.gyroflow.Stamp360Test(video_in, log=log, time_base=0x0123456789ABCDEF).set_output()  # noqa: F821
