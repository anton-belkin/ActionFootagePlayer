-- Ctrl+e's watch on the Gyroflow project (mpv/scripts/gyroflow/projectwatch.lua): when a save in Gyroflow is taken up.
-- The project's file in a temporary folder, its mtime set by hand (mpv's are whole seconds).
-- From the repository's folder: luajit tests/projectwatch_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
local pw = require 'projectwatch'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local project = dir .. "/clip.gyroflow"

-- mp.utils.file_info's stand-in: the size from the file, the mtime as set here (whole seconds, as mpv's)
local mtime = 1795126018
local utils = {
    file_info = function(path)
        local f = io.open(path, "rb")
        if not f then return nil end
        local size = f:seek("end")
        f:close()
        return { is_file = true, mtime = mtime, size = size }
    end,
}
local function save(text, t)
    local f = io.open(project, "wb")
    f:write(text)
    f:close()
    mtime = t
end
-- One tick of the watch at `now`: what it does
local function tick(watch, now)
    local action, version = pw.step(watch, pw.version(utils, project, now))
    return action, version
end

-- An existing project edited: taken up once it stayed the same for a tick, once
do
    save('{"smoothness":0.75}', 1000)
    local watch = { project = project, video = "clip.MP4", version = pw.version(utils, project, 2000) }
    expect("untouched", tick(watch, 2000), nil)
    save('{"smoothness":0.20}', 2001)
    expect("being written", tick(watch, 2001), nil)
    local action, version = tick(watch, 2001)
    expect("saved: reloads", action, "reload")
    expect("its version", version, "2001/" .. #'{"smoothness":0.20}' .. "#1")
    expect("then nothing", tick(watch, 2002), nil)
    -- Out of its fresh seconds, not read any more: still the same save
    expect("aged", tick(watch, 2010), nil)
    expect("aged, again", tick(watch, 2011), nil)
end

-- Saved twice in the same second, the same size (mpv's mtime and the size can't tell them apart): both taken up, each
-- with a version of its own
do
    save('{"smoothness":0.75}', 3000)
    local watch = { project = project, video = "clip.MP4", version = pw.version(utils, project, 3100) }
    save('{"smoothness":0.20}', 3101)
    tick(watch, 3101)
    local a1, v1 = tick(watch, 3101)
    expect("first save", a1, "reload")
    save('{"smoothness":0.95}', 3101)
    expect("second save, same second and size: noticed", tick(watch, 3101), nil)
    local a2, v2 = tick(watch, 3101)
    expect("second save: reloads", a2, "reload")
    expect("a version of its own", v1 ~= v2, true)
    expect("then nothing", tick(watch, 3102), nil)
    -- Saved again in a later second (Cmd+S with nothing changed): a new stamp, taken up as a save
    save('{"smoothness":0.95}', 3102)
    tick(watch, 3102)
    local a3 = tick(watch, 3102)
    expect("a new time: reloads", a3, "reload")
end

-- A project appearing beside a clip that had none: found once it stays the same
do
    os.remove(project)
    local watch = { project = nil, video = "clip.MP4", version = nil }
    expect("none yet", pw.step(watch, nil), nil)
    save('{"smoothness":0.75}', 4000)
    expect("appeared", tick(watch, 4000), nil)
    expect("stayed: found", tick(watch, 4000), "found")
end

-- A file being written as it's read: each tick differs, nothing taken until it stays the same
do
    save('{"a":1}', 5000)
    local watch = { project = project, version = pw.version(utils, project, 5000) }
    save('{"a":1,"b":', 5001)
    expect("half written", tick(watch, 5001), nil)
    save('{"a":1,"b":2}', 5001)
    expect("written on", tick(watch, 5001), nil)
    expect("stayed", tick(watch, 5001), "reload")
end

-- A project left alone isn't read: only its stamp
do
    save('{"a":1}', 6000)
    expect("fresh: read", pw.version(utils, project, 6001).content, '{"a":1}')
    expect("old: not read", pw.version(utils, project, 6100).content, nil)
    expect("no file", pw.version(utils, dir .. "/none.gyroflow", 6100), nil)
end

os.execute("rm -rf '" .. dir .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
