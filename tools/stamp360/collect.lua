-- For the 360 stamp's test (stamp360.py): from the first frame on, paused, a screenshot of each of `frames` frames (as
-- the video's own size, with the user shaders), then what mpv had, as JSON in `out`/props.json, then quits
local opts = { out = "", frames = 6 }
require("mp.options").read_options(opts, "stamp360")
local utils = require("mp.utils")

local count, props, last = 0, {}, nil

local function finish()
    local f = io.open(opts.out .. "/props.json", "w")
    f:write(utils.format_json(props))
    f:close()
    mp.command("quit")
end

local function shoot()
    local path = string.format("%s/shot_%03d.png", opts.out, count)
    mp.commandv("screenshot-to-file", path, "video")
    props[#props + 1] = {
        shot = path, pts = mp.get_property_number("time-pos"),
        hwdec = mp.get_property("hwdec-current"),
        decoded = mp.get_property_native("video-dec-params"),
        params = mp.get_property_native("video-params"),
        -- after the filters: what gpu-next gets
        filtered = mp.get_property_native("video-out-params"),
        vo = mp.get_property("current-vo"),
    }
    count = count + 1
    if count >= opts.frames then
        mp.add_timeout(1.5, finish)
    else
        mp.add_timeout(0.8, function() mp.command("frame-step") end)
    end
end

-- A new frame on screen, paused: shoot it once it's drawn
mp.observe_property("time-pos", "number", function(_, t)
    if t == nil or t == last then return end
    last = t
    mp.add_timeout(0.5, shoot)
end)
-- Never hangs a run
mp.add_timeout(60, finish)
