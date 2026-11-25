-- framing: the output's frame, for the gyroflow plugin (main.lua wires it): its aspect, orientation, turn and
-- projection, the size it renders at, mpv's own turn and crop where the warp doesn't make it, and the Frame's messages
local mp = require 'mp'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, INTERPOLATION_FAST, INTERPOLATION_SLOW, SLOW_MAX_FPS
-- The other parts' functions, looked up in W as they're called
local function filtering(...) return W.filtering(...) end
local function has_project(...) return W.has_project(...) end
local function publish(...) return W.publish(...) end
local function set_lens(...) return W.set_lens(...) end
local function settings_changed(...) return W.settings_changed(...) end
local function sticks(...) return W.sticks(...) end
-- Set further down, or as it installs
local mpv_frame

-- The output's frame, this clip's (the Clip tab's; saved with its view): turned clockwise by 0, 90, 180 or 270
-- degrees, then cut to an aspect ("original": the frame's own). Where it's stabilized, Gyroflow's output size and
-- rotation (vs-gyroflow), its zoom fitting the picture into it; elsewhere mpv's video-rotate and video-crop. A clip
-- with a project has the project's own unless changed here, for viewing
local ASPECTS = { original = true, ["16:9"] = true, ["4:3"] = true, ["5:4"] = true, ["1:1"] = true, ["4:5"] = true, ["3:4"] = true, ["9:16"] = true }
-- Those, or any W:H
local function aspect_ok(a)
    return a ~= nil and (ASPECTS[a] == true or tostring(a):match("^%d+:%d+$") ~= nil)
end
-- The frame's ratio is kept landscape-wise (16:9) with an orientation: as-is (as the source is), landscape, portrait. An
-- aspect from before (9:16: the ratio as it showed) is that ratio turned, portrait; another W:H landscape; original as is
local ORIENTATIONS = { ["as-is"] = true, landscape = true, portrait = true }
local function split_aspect(aspect, orientation)
    if not aspect_ok(aspect) then aspect = "original" end
    local w, h = tostring(aspect):match("^(%d+):(%d+)$")
    if w and tonumber(w) < tonumber(h) then return h .. ":" .. w, "portrait" end
    if ORIENTATIONS[orientation] then return aspect, orientation end
    return aspect, w and "landscape" or "as-is"
end
-- The W:H the frame shows for a source w x h (upright), or original (workspace's transform.lua concrete, the same)
local function concrete(aspect, orientation, w, h)
    local landscape = not (w and h) or w >= h
    if aspect == nil or aspect == "original" then
        if orientation == "as-is" or orientation == nil or not (w and h) then return "original" end
        if (orientation == "portrait") == (w < h) or w == h then return "original" end
        local function gcd(a, b) while b ~= 0 do a, b = b, a % b end return a end
        local g = gcd(math.floor(w), math.floor(h))
        return string.format("%d:%d", math.floor(h) / g, math.floor(w) / g)
    end
    local a, b = tostring(aspect):match("^(%d+):(%d+)$")
    if not a then return "original" end
    local portrait = orientation == "portrait" or (orientation ~= "landscape" and not landscape)
    if portrait and a ~= b then return b .. ":" .. a end
    return a .. ":" .. b
end
-- The frame's W:H as it shows now (further down, once the source's size is known)
local shown_aspect

-- And the output's lens where it's stabilized, Gyroflow's output projection: linear (rectilinear, straight lines
-- straight), or a fisheye, its edges more and more compressed: stereographic, equidistant, equisolid, orthographic. For
-- Wide footage above water, too wide for linear: its center squeezed, its edges thin
local PROJECTIONS = { linear = 0, stereographic = 1, equidistant = 2, equisolid = 3, orthographic = 4 }

-- The size of a w x h frame turned and cut to the aspect: its largest centered part of that aspect, each side
-- rounded down to even (vs-gyroflow's Frame::size, which renders it)
local function frame_size(w, h, f)
    f = f or S.frame
    if f.rotation == 90 or f.rotation == 270 then w, h = h, w end
    local aw, ah = tostring(f.aspect):match("^(%d+):(%d+)$")
    aw, ah = tonumber(aw), tonumber(ah)
    if aw and ah and aw > 0 and ah > 0 then
        if w * ah > h * aw then w = math.floor(h * aw / ah) else h = math.floor(w * ah / aw) end
    end
    return math.max(2, math.floor(w / 2) * 2), math.max(2, math.floor(h / 2) * 2)
end

-- The shape of the stabilized frame, w, h (nil: not known yet): the project's output, or the source turned and cut as
-- the frame says. The window has it while the warp's table is written (the table's own output then)
local function output_shape(source)
    local w, h = source.width, source.height
    if not w or not h or w <= 0 or h <= 0 then return nil end
    local ow, oh = source.out_width or w, source.out_height or h
    -- The file's own turn (its display matrix: a camera held upright) comes first, then the frame's: the warp renders
    -- it upright (vs-gyroflow's Frame), and mpv doesn't turn it again. A project has its own, unless the frame changes it
    local turn = (source.rotation or 0) % 360
    local fa = shown_aspect()
    if S.frame.rotation ~= 0 or fa ~= "original" or (turn ~= 0 and source.kind ~= "project") then
        ow, oh = frame_size(w, h, { rotation = (turn + S.frame.rotation) % 360, aspect = fa })
    end
    return ow, oh
end

local function interpolation(source)
    local fps = tonumber(source.fps)
    if fps and fps > 0 and fps <= SLOW_MAX_FPS then return INTERPOLATION_SLOW end
    return INTERPOLATION_FAST
end

-- The upright source's ratio, W:H reduced
local function source_aspect()
    local src = S.current_source
    local w, h = src and src.width, src and src.height
    if not (w and h and w > 0 and h > 0) then
        local d = mp.get_property_native("video-dec-params")
        if type(d) ~= "table" or not d.w or not d.h then return nil end
        w, h = d.w, d.h
        if ((tonumber(d.rotate) or 0) % 180) == 90 then w, h = h, w end
    elseif (((src.rotation or 0) + S.frame.rotation) % 180) == 90 then
        w, h = h, w
    end
    local function gcd(a, b) while b ~= 0 do a, b = b, a % b end return a end
    local g = gcd(math.floor(w), math.floor(h))
    return string.format("%d:%d", math.floor(w) / g, math.floor(h) / g)
end

shown_aspect = function()
    local w, h = tostring(source_aspect() or ""):match("^(%d+):(%d+)$")
    return concrete(S.frame.aspect, S.frame.orientation, tonumber(w), tonumber(h))
end

-- mpv's own turn and crop of the frame where the stabilization doesn't make it (off, or no motion data): the crop in
-- the decoded frame's own coordinates, before the turn
mpv_frame = function()
    if not mp.get_property("path") then return end
    -- Stabilized, the warp's frame is upright already (the file's turn and the frame's): mpv turns it no more
    local rotation, crop = "no", ""
    if not filtering() then
        -- mpv's turn is added to the file's own (its display matrix), and the crop is in the decoded frame's coordinates
        rotation = tostring(S.frame.rotation)
        local d = mp.get_property_native("video-dec-params")
        local fa = shown_aspect()
        if fa ~= "original" and type(d) == "table" and d.w and d.h then
            local total = ((tonumber(d.rotate) or 0) + S.frame.rotation) % 360
            local fw, fh = frame_size(d.w, d.h, { rotation = total, aspect = fa })
            if total == 90 or total == 270 then fw, fh = fh, fw end
            fw, fh = math.min(fw, d.w), math.min(fh, d.h)
            -- The whole frame (9:16 turned a quarter): nothing to crop
            if fw < d.w - 1 or fh < d.h - 1 then crop = string.format("%dx%d+%d+%d", fw, fh, math.floor((d.w - fw) / 4) * 2, math.floor((d.h - fh) / 4) * 2) end
        end
    end
    if mp.get_property("file-local-options/video-rotate") ~= rotation then
        mp.set_property("file-local-options/video-rotate", rotation)
    end
    if (mp.get_property("file-local-options/video-crop") or "") ~= crop then mp.set_property("file-local-options/video-crop", crop) end
end

local ROTATION_LABELS = { [0] = "not turned", [90] = "turned 90° right", [180] = "turned 180°", [270] = "turned 90° left" }

-- The frame turned and cut to an aspect, for this clip: set up again where it's stabilized (its output size changes)
local PROJECTION_LABELS = { linear = "linear lens", stereographic = "stereographic fisheye", equidistant = "equidistant fisheye",
                            equisolid = "equisolid fisheye", orthographic = "orthographic fisheye", project = "the project's lens" }
local ORIENTATION_LABELS = { ["as-is"] = "as the source is", landscape = "landscape", portrait = "portrait" }
local function set_frame(aspect, rotation, projection, orientation)
    projection = projection or S.frame.projection
    orientation = orientation or S.frame.orientation
    if aspect == S.frame.aspect and rotation == S.frame.rotation and projection == S.frame.projection and orientation == S.frame.orientation then return end
    S.frame = { aspect = aspect, rotation = rotation, projection = projection, orientation = orientation }
    local shown = shown_aspect()
    local message = "Frame: " .. (aspect == "original" and "its own ratio" or aspect) .. ", " .. ORIENTATION_LABELS[orientation]
        .. (shown ~= "original" and shown ~= aspect and (" (" .. shown .. ")") or "") .. ", " .. ROTATION_LABELS[rotation]
        .. ", " .. PROJECTION_LABELS[projection]
    if filtering() then settings_changed() end
    mp.osd_message(message, 2)
    mpv_frame()
    publish()
end

-- What a Gyroflow project has is changed only there
local function project_owns(what)
    if not has_project(S.current_source) then return false end
    mp.osd_message("Gyroflow: " .. what .. " as defined in the Gyroflow project (Ctrl+e edits it in Gyroflow)", 2.5)
    return true
end

-- The output's projection; over a Gyroflow project too, for viewing and saved with the clip ("project": the project's
-- own), and not the session's then
local function set_projection(p)
    local project = has_project(S.current_source)
    if not (PROJECTIONS[p] or (p == "project" and project)) then return end
    set_frame(S.frame.aspect, S.frame.rotation, p)
    if not project then sticks("projection", p) end
    publish()
end

-- The Lens drop-down's pick (the side panel's): as-recorded, or corrected in a projection (linear, stereographic,
-- equidistant, equisolid, orthographic; "project": a Gyroflow project's own), both changed at once
function M.lens(k)
    if k == "as-recorded" or k == "corrected" then return set_lens(k) end
    local project = has_project(S.current_source)
    if not (PROJECTIONS[k] or (k == "project" and project)) then return end
    if S.lens_mode == "corrected" or not S.current_source then
        if S.lens_mode ~= "corrected" then set_lens("corrected") end
        return set_projection(k)
    end
    -- From as recorded: the warp written once, for both
    S.lens_mode, S.frame.projection = "corrected", k
    if not project then sticks("lens", "corrected"); sticks("projection", k) end
    settings_changed()
    mpv_frame()
    publish()
end

-- The ratio, landscape-wise (original, 16:9, 4:3, 5:4, 1:1); a portrait one (9:16) is that ratio turned, portrait
function M.aspect(a)
    if not aspect_ok(a) or project_owns("the frame") then return end
    local ratio, orientation = split_aspect(a, S.frame.orientation)
    set_frame(ratio, S.frame.rotation, nil, orientation)
    sticks("aspect", ratio)
    sticks("orientation", orientation)
    publish()
end

-- as-is (as the source is), landscape, portrait
function M.orientation(v)
    if not ORIENTATIONS[v] or project_owns("the frame") then return end
    set_frame(S.frame.aspect, S.frame.rotation, nil, v)
    sticks("orientation", v)
    publish()
end

-- The source turned as it was recorded (input): right, left, a quarter turn clockwise or back; 0, 90, 180, 270: that
-- turn. Written into the source's own file at once (the workspace plugin), for all its moments and clips
function M.rotate(v)
    local r = v == "right" and (S.frame.rotation + 90) % 360 or v == "left" and (S.frame.rotation + 270) % 360 or tonumber(v)
    if not ROTATION_LABELS[r] or project_owns("the turn") then return end
    set_frame(S.frame.aspect, r)
    mp.commandv("script-message-to", "workspace", "input-set", "rotation", tostring(r))
end

function M.install(w)
    W = w
    S = W.S
    INTERPOLATION_FAST, INTERPOLATION_SLOW, SLOW_MAX_FPS = W.INTERPOLATION_FAST, W.INTERPOLATION_SLOW, W.SLOW_MAX_FPS
    S.frame = { aspect = "original", orientation = "as-is", rotation = 0, projection = "linear" }
    W.PROJECTIONS, W.interpolation, W.mpv_frame, W.output_shape, W.project_owns, W.set_projection = PROJECTIONS, interpolation, mpv_frame, output_shape, project_owns, set_projection
    W.shown_aspect, W.source_aspect, W.split_aspect = shown_aspect, source_aspect, split_aspect
end

return M
