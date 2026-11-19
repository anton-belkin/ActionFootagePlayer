-- live-photo: photos shown as a phone shows them (docs/live-photos.md).
--
-- A photo (HEIC, JPEG, PNG…) opens as the picture, on the whole window, and stays (no slideshow). A live one (a Live
-- Photo: the photo and its .mov; a Motion Photo: the video inside the JPEG) has the Live icon in its top-left corner:
-- the mouse over it, or Space held, plays its live part with its sound; let go, and the picture is back. Photos taken
-- in a row, whose live parts overlap by when they were taken, play on into each other while it's held: at the end of
-- one's video, the next one's goes on from the same moment, until the run ends; let go, and the player rests on the
-- photo it got to.
--
-- The video is the photo file's own extra track (video-add), so the playlist entry stays the photo: Previous/Next go
-- on through photos and clips as through any playlist. A Live Photo's .mov is taken out of the playlist when its photo
-- is in it. What doesn't apply to a still (stabilization, moments, exports, crop, cuts, marks) asks
-- user-data/afp/photo and does nothing.
--
-- A photo's link from the Library (<name>.afplink.jpg: a frame of a clip) plays the same way: the workspace plugin opens
-- it as its clip's part around the frame (an EDL, as a moment), paused at the frame; the live part is that part,
-- 1.5 s on each side. Links to the same clip in the playlist whose parts overlap make one run: the part opened is
-- then all of theirs, so it plays on without a break; let go, and the player rests on the link it got to. Its whole
-- clip is a button away (the side panel's, the menu's Open the whole clip: the workspace plugin's open-original).
--
-- HDR (macOS): a photo with a gain map (an iPhone's HEIC, an Ultra HDR JPEG, the player's own) is shown as macOS
-- shows it in HDR. mpv decodes only the photo's SDR image, so the gyroflow plugin's afp-photo (--hdr-still: Core
-- Image applies the gain map, as Preview and Photos do) writes the HDR rendition once, a 10-bit PQ HEIF in the cache
-- (<Caches>/ActionFootagePlayer/hdr-stills); it's added to the file as another picture track and shown in its place,
-- the display switched to HDR for it (target PQ, as the dlogm plugin does for HDR video). The SDR image shows until
-- it's ready (0.2-0.5 s the first time), and on an SDR screen (user-data/dlogm/screen: as Preview, the photo's own
-- SDR image there). Not on Windows or Linux yet: the photo's SDR image.
--
-- Script messages (what the hover and Space send; tests drive them over IPC):
--   live-photo-view play       the live part plays (the chain on)
--   live-photo-view stop       back to the picture (or to the photo the chain got to)
--   live-photo-layout w h s …  tests: the icon laid out for a window that size (vo=null has no OSD)
--   live-photo-dump <path>     what it knows, as JSON (tests)

local mp = require 'mp'
local utils = require 'mp.utils'
package.path = mp.get_script_directory() .. "/?.lua;" .. package.path
local live = require 'live'
-- The workspace plugin's reader of a photo's link (afp:link in its XMP), beside this one in the app
local read_photo_link = nil
do
    local plugins = mp.get_script_directory():match("^(.*)[/\\][^/\\]+[/\\]?$")
    if plugins then
        package.path = package.path .. ";" .. utils.join_path(utils.join_path(plugins, "workspace"), "?.lua")
        local ok, l = pcall(require, "link")
        if ok and type(l) == "table" then read_photo_link = l.photo end
    end
end

local AHEAD, BEHIND = 12, 4 -- playlist entries described around a photo, for its chain

local info = nil          -- the photo playing: live.describe's, with .path, or nil (not a photo)
local described = {}      -- path -> live.describe's (false: not a photo)
local playing = nil       -- { run = {…}, step = i, tracks = { [path] = { vid, aid } }, by = "hover"|"key"|"message" }
local hovered = false
local icon_rect = nil     -- { x0, y0, x1, y1 } in OSD pixels
local overlay = mp.create_osd_overlay("ass-events")
overlay.z = 5
local image_vid = nil     -- the photo's own picture track
local test_dims = nil     -- live-photo-layout's window, for a player without one (tests: vo=null has no OSD)
local hdr = { vid = nil, path = nil, target = nil } -- the HDR rendition's track for the photo `path`; target: the
                                                    -- options it changed, as they were

local function read(path)
    if live.is_link(path) then
        local raw = read_photo_link and read_photo_link(path)
        return live.describe_link(raw and utils.parse_json(raw))
    end
    return live.describe(path)
end
local function describe(path)
    if described[path] == nil then
        local ok, d = pcall(read, path)
        described[path] = ok and d or false
        if not ok then mp.msg.warn("couldn't read " .. path .. ": " .. tostring(d)) end
        if type(described[path]) == "table" then described[path].path = path end
    end
    return described[path] or nil
end

local function playlist_paths()
    local list = mp.get_property_native("playlist") or {}
    local dir = utils.split_path(mp.get_property("path") or "")
    local out = {}
    for i, e in ipairs(list) do
        local f = e.filename
        -- Relative names (mpv's own playlist of a folder keeps them as given)
        if f and not f:match("^/") and not f:match("^%a:[\\/]") and not f:find("://", 1, true) then f = utils.join_path(dir, f) end
        out[i] = f
    end
    return out
end

local function index_of(path)
    for i, p in ipairs(playlist_paths()) do if p == path then return i end end
    return nil
end

-- The described photos around a playlist position (described now, as needed)
local function around(path)
    local list = playlist_paths()
    local at
    for i, p in ipairs(list) do if p == path then at = i break end end
    local out = {}
    if not at then return out end
    for i = math.max(1, at - BEHIND), math.min(#list, at + AHEAD) do
        local d = (live.is_photo(list[i]) or live.is_link(list[i])) and describe(list[i]) or nil
        if d then out[#out + 1] = d end
    end
    return out
end

-- The same, a few at a time from a timer: ready before the mouse comes
local warming = nil
local function warm(path)
    if warming then warming:kill() end
    local list = playlist_paths()
    local at
    for i, p in ipairs(list) do if p == path then at = i break end end
    if not at then return end
    local todo = {}
    for i = at + 1, math.min(#list, at + AHEAD) do todo[#todo + 1] = list[i] end
    for i = at - 1, math.max(1, at - BEHIND), -1 do todo[#todo + 1] = list[i] end
    warming = mp.add_periodic_timer(0.02, function()
        local p = table.remove(todo, 1)
        if not p then warming:kill() warming = nil return end
        if live.is_photo(p) or live.is_link(p) then describe(p) end
    end)
end

-- ---------- the icon ----------

-- The Live icon as macOS draws it (the Library's tiles: library/ui/tiles.js): a dot, a ring and 24 short rays around
-- them, in a 24-unit box at (x, y), `size` pixels
local function circle(cx, cy, r)
    local k = 0.5523 * r
    return string.format("m %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f ",
        cx + r, cy, cx + r, cy + k, cx + k, cy + r, cx, cy + r, cx - k, cy + r, cx - r, cy + k, cx - r, cy,
        cx - r, cy - k, cx - k, cy - r, cx, cy - r, cx + k, cy - r, cx + r, cy - k, cx + r, cy)
end
local function icon_path(x, y, size)
    local u = size / 24
    local function px(v) return v * u end
    local p = {}
    -- The dot, the ring as a band between two circles (the inner one drawn the other way round)
    p[#p + 1] = circle(x + px(12), y + px(12), px(2.4))
    local function ring(r0, r1)
        local cx, cy = x + px(12), y + px(12)
        local s = circle(cx, cy, px(r1))
        -- inner, counterclockwise
        local r, k = px(r0), 0.5523 * px(r0)
        s = s .. string.format("m %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f ",
            cx + r, cy, cx + r, cy - k, cx + k, cy - r, cx, cy - r, cx - k, cy - r, cx - r, cy - k, cx - r, cy,
            cx - r, cy + k, cx - k, cy + r, cx, cy + r, cx + k, cy + r, cx + r, cy + k, cx + r, cy)
        return s
    end
    p[#p + 1] = ring(4.6, 6.2)
    -- The rays: thin quads from r 8.6 to 10.6
    local w = px(0.65)
    for i = 0, 23 do
        local a = i * math.pi / 12
        local c, s = math.cos(a), math.sin(a)
        local function pt(r, side) return x + px(12 + r * c) - side * w * s, y + px(12 + r * s) + side * w * c end
        local ax, ay = pt(8.6, 1)
        local bx, by = pt(10.6, 1)
        local cx, cy = pt(10.6, -1)
        local dx, dy = pt(8.6, -1)
        p[#p + 1] = string.format("m %.2f %.2f l %.2f %.2f %.2f %.2f %.2f %.2f ", ax, ay, bx, by, cx, cy, dx, dy)
    end
    return table.concat(p)
end

local function scale()
    return mp.get_property_number("display-hidpi-scale", 1) or 1
end

local function draw()
    if not (info and info.live) then
        icon_rect = nil
        overlay.data = ""
        overlay:update()
        return
    end
    local d = mp.get_property_native("osd-dimensions")
    if (not d or d.w <= 0 or d.h <= 0) and test_dims then d = test_dims end
    if not d or d.w <= 0 or d.h <= 0 then return end
    local s = test_dims and d == test_dims and test_dims.scale or scale()
    local x0, y0 = d.ml, d.mt
    -- Below the macOS title bar where the picture starts under it
    local top = math.max(y0, 28 * s)
    local size, pad = 18 * s, 6 * s
    local fs = 13 * s
    local bx, by = x0 + 14 * s, top + 12 * s
    local text_w = 30 * s -- "LIVE" at 13 pt, bold, spaced
    local w, h = pad + size + 5 * s + text_w + pad + 2 * s, size + 2 * pad
    icon_rect = { x0 = bx, y0 = by, x1 = bx + w, y1 = by + h }
    local on = playing ~= nil
    local back = on and "&HFFFFFF&" or "&H000000&"
    local fore = on and "&H202020&" or "&HFFFFFF&"
    local alpha = on and "&H20&" or (hovered and "&H50&" or "&H70&")
    local ev = {}
    -- The pill
    local r = h / 2
    local k = 0.5523 * r
    ev[#ev + 1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c%s\\1a%s\\p1}m %.2f %.2f l %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f l %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f b %.2f %.2f %.2f %.2f %.2f %.2f{\\p0}",
        back, alpha,
        bx + r, by, bx + w - r, by,
        bx + w - r + k, by, bx + w, by + r - k, bx + w, by + r,
        bx + w, by + r + k, bx + w - r + k, by + h, bx + w - r, by + h,
        bx + r, by + h,
        bx + r - k, by + h, bx, by + r + k, bx, by + r,
        bx, by + r - k, bx + r - k, by, bx + r, by)
    ev[#ev + 1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c%s\\1a&H00&\\p1}%s{\\p0}", fore, icon_path(bx + pad, by + pad, size))
    ev[#ev + 1] = string.format("{\\an4\\pos(%.2f,%.2f)\\bord0\\shad0\\1c%s\\1a&H00&\\fs%.1f\\b1\\fsp%.1f\\fnHelvetica Neue}LIVE",
        bx + pad + size + 5 * s, by + h / 2, fore, fs, 0.8 * s)
    overlay.res_x, overlay.res_y = d.w, d.h
    overlay.data = table.concat(ev, "\n")
    overlay:update()
end

-- ---------- playing ----------

local function tracks()
    return mp.get_property_native("track-list") or {}
end

-- The tracks a photo's video added to this file has: { vid, aid } or nil while it isn't there
local function tracks_of(d)
    local t = { }
    for _, tr in ipairs(tracks()) do
        if tr.external and tr["external-filename"] == d.url then
            if tr.type == "video" and not t.vid then t.vid = tr.id end
            if tr.type == "audio" and not t.aid then t.aid = tr.id end
        end
    end
    return t.vid and t or nil
end

-- A photo's video added to this file (once): its tracks then come into track-list
local adding = {}
local function add_video(d, done)
    if tracks_of(d) then if done then done(tracks_of(d)) end return end
    if adding[d.url] then
        if done then adding[d.url][#adding[d.url] + 1] = done end
        return
    end
    adding[d.url] = { done }
    mp.command_native_async({ name = "video-add", url = d.url, flags = "auto", title = "Live" }, function(ok, _, err)
        local waiting = adding[d.url] or {}
        adding[d.url] = nil
        if not ok then mp.msg.warn("couldn't add the live part " .. d.url .. ": " .. tostring(err)) end
        local t = tracks_of(d)
        for _, f in ipairs(waiting) do f(t) end
    end)
end

local function publish()
    local p = nil
    if info then
        p = { live = info.live == true, kind = info.kind, playing = playing ~= nil, file = info.path, span = info.span,
              step = playing and playing.step or 0, run = playing and #playing.run or 0,
              at = playing and playing.run[playing.step].path or nil }
    end
    if p then mp.set_property_native("user-data/afp/photo", p)
    else pcall(mp.del_property, "user-data/afp/photo") end
end

-- ---------- HDR ----------

local MACOS = mp.get_property("platform") == "darwin"
-- The gyroflow plugin's afp-photo, beside this plugin in the app
local function afp_photo()
    if not MACOS then return nil end
    local plugins = mp.get_script_directory():match("^(.*)[/\\][^/\\]+[/\\]?$")
    local exe = plugins and utils.join_path(utils.join_path(plugins, "gyroflow"), "afp-photo")
    local info = exe and utils.file_info(exe)
    return info and info.is_file and exe or nil
end
local function cache_dir()
    local home = os.getenv("AFP_HOME")
    if home and home ~= "" then return utils.join_path(utils.join_path(home, "cache"), "hdr-stills") end
    local h = os.getenv("HOME")
    return h and utils.join_path(h, "Library/Caches/ActionFootagePlayer/hdr-stills")
end
-- The rendition's name: the photo's path, size and time (a changed photo, a new one)
local function cache_name(path)
    local info = utils.file_info(path)
    if not info then return nil end
    local s = path .. "|" .. tostring(info.size) .. "|" .. tostring(info.mtime)
    local a, b = 5381, 0
    for i = 1, #s do
        local c = s:byte(i)
        a = (a * 33 + c) % 4294967296
        b = (b * 31 + c + 7) % 4294967296
    end
    return string.format("%08x%08x.heic", a, b)
end
-- An HDR screen (the dlogm plugin's; unknown counts as HDR, as there: macOS tone-maps for an SDR one)
local function hdr_screen()
    return mp.get_property_native("user-data/dlogm/screen") ~= "sdr"
end

-- The rendition's frames labelled as what they are (vf format, labelled live-photo-hdr): mpv puts a HEIF's 512-pixel
-- tiles together into one picture without their PQ, BT.2020 and full range, and would show it as SDR
local HDR_LABEL = "live-photo-hdr"
local function hdr_label(on)
    local vf = mp.get_property_native("vf") or {}
    local at
    for i, f in ipairs(vf) do if f.label == HDR_LABEL then at = i end end
    if on and not at then
        vf[#vf + 1] = { name = "format", label = HDR_LABEL,
                        params = { colorlevels = "full", colormatrix = "bt.2020-ncl", gamma = "pq", primaries = "bt.2020" } }
    elseif not on and at then
        table.remove(vf, at)
    else
        return
    end
    mp.set_property_native("file-local-options/vf", vf)
end

-- The display in HDR for the rendition (PQ, as it is), or back as it was
local TARGET = { ["target-colorspace-hint"] = "yes", ["target-trc"] = "pq", ["target-prim"] = "bt.2020", ["target-peak"] = "1000" }
local function hdr_target(on)
    if on and not hdr.target then
        hdr.target = {}
        for k, v in pairs(TARGET) do
            hdr.target[k] = mp.get_property("file-local-options/" .. k)
            mp.set_property("file-local-options/" .. k, v)
        end
    elseif not on and hdr.target then
        for k, v in pairs(hdr.target) do mp.set_property("file-local-options/" .. k, v) end
        hdr.target = nil
    end
end

-- The still's track: the HDR rendition on an HDR screen once it's there, else the photo's own picture
local function still_vid()
    if hdr.vid and hdr.path == (info and info.path) and hdr_screen() then return hdr.vid, true end
    return image_vid, false
end

-- The picture again: its own track (or its HDR rendition), the sound off
local function show_still()
    mp.set_property_bool("pause", true)
    local vid, is_hdr = still_vid()
    hdr_label(is_hdr)
    if vid then mp.set_property_native("vid", vid) end
    hdr_target(is_hdr)
    mp.set_property("aid", "no")
end

-- The HDR rendition of the photo shown: written once (afp-photo), added as a picture track, shown when the still is
local function want_hdr(d)
    if not (d and d.kind ~= "link" and hdr_screen()) or d.no_hdr then return end
    local exe, dir = afp_photo(), cache_dir()
    local name = exe and dir and cache_name(d.path)
    if not name then return end
    local out = utils.join_path(dir, name)
    local function add()
        mp.command_native_async({ name = "video-add", url = out, flags = "auto", title = "HDR" }, function(ok)
            if not ok or info ~= d then return end
            for _, tr in ipairs(mp.get_property_native("track-list") or {}) do
                if tr.type == "video" and tr["external-filename"] == out then hdr.vid, hdr.path = tr.id, d.path end
            end
            mp.msg.verbose("HDR still: " .. out)
            if not playing then show_still() end
        end)
    end
    if utils.file_info(out) then return add() end
    mp.command_native_async({ name = "subprocess", args = { "mkdir", "-p", dir }, playback_only = false }, function()
        mp.command_native_async({ name = "subprocess", args = { exe, "--hdr-still", d.path, "--out", out }, playback_only = false,
                                 capture_stderr = true }, function(_, r)
            if r and r.status == 3 then d.no_hdr = true return end
            if not (r and r.status == 0) then
                mp.msg.warn("no HDR still for " .. d.path .. ": " .. tostring(r and (r.stderr or r.error_string)))
                return
            end
            if info == d then add() end
        end)
    end)
end

-- Another screen (the window moved, a screen's HDR switched): the still that fits it
mp.observe_property("user-data/dlogm/screen", "native", function()
    if not info or info.kind == "link" or playing then return end
    if hdr_screen() and not (hdr.vid and hdr.path == info.path) then want_hdr(info) end
    show_still()
end)

local function finish(rest_on)
    if not playing then return end
    mp.msg.verbose("back to the picture, step " .. playing.step)
    local reached = playing.run[playing.step]
    local link = playing.link
    playing = nil
    if link then
        -- A link: back to its frame in the part (or the link the run got to, below)
        mp.set_property_bool("pause", true)
        if reached == info then mp.commandv("seek", string.format("%.4f", info.taken - info.span[1]), "absolute+exact") end
    else
        show_still()
    end
    publish()
    draw()
    -- Let go on another photo of the run: that photo
    if rest_on and reached ~= info then
        local i = index_of(reached.path)
        if i then mp.commandv("playlist-play-index", tostring(i - 1)) end
    end
end

local function play_step(i, at)
    local d = playing.run[i]
    local t = tracks_of(d)
    mp.msg.verbose(string.format("step %d: %s at %.3f s (tracks %s)", i, d.path, at, t and tostring(t.vid) or "none"))
    if not t then return false end
    playing.step = i
    -- Until the seek lands, the time and the end are the last video's
    playing.settling = true
    hdr_label(false)
    mp.set_property_native("vid", t.vid)
    mp.set_property("aid", t.aid and tostring(t.aid) or "no")
    mp.commandv("seek", string.format("%.4f", math.max(0, at)), "absolute+exact")
    mp.set_property_bool("pause", false)
    -- The next one's video ready before this one ends (and the photos around it described)
    local after = live.next_in_chain(d, around(d.path))
    if after then
        playing.run[i + 1] = after
        add_video(after)
    else
        playing.run[i + 1] = nil
    end
    publish()
    return true
end

local function start(by)
    if not (info and info.live) or playing then return end
    if info.kind == "link" then
        -- The part opened is the run's (the workspace plugin made it so): from its start, no other file
        playing = { run = info.run or { info }, step = 1, by = by, link = true, settling = true }
        mp.commandv("seek", "0", "absolute+exact")
        mp.set_property_bool("pause", false)
        publish()
        draw()
        return
    end
    playing = { run = { info }, step = 1, by = by }
    draw()
    add_video(info, function(t)
        if not playing or playing.run[1] ~= info then return end
        if not t then playing = nil draw() return end
        play_step(1, 0)
    end)
end

local function stop(by)
    if not playing then return end
    if by and playing.by ~= by and by ~= "message" then return end
    finish(true)
end

-- At the end of a step's video: the next one's, from the same moment, or the end of the run
local function at_end()
    if not playing then return end
    local i = playing.step
    local cur, nxt = playing.run[i], playing.run[i + 1]
    if nxt and tracks_of(nxt) then
        play_step(i + 1, live.carry_on_at(cur, nxt))
        return
    end
    -- The run's end while still held: the photo it got to
    finish(true)
end

mp.register_event("playback-restart", function()
    if playing then playing.settling = false end
end)

mp.observe_property("time-pos", "number", function(_, pos)
    if not playing or playing.settling or not pos then return end
    if playing.link then
        -- Which link of the run the clip is at now, and the part's end
        local at = live.at_time(playing.run, info.span[1] + pos)
        if at ~= playing.step then playing.step = at publish() end
        if pos >= info.span[2] - info.span[1] - 0.07 then finish(true) end
        return
    end
    local cur = playing.run[playing.step]
    -- Its last frame (a frame or two before the length the container gives)
    if pos >= cur.duration - 0.07 then at_end() end
end)
mp.observe_property("eof-reached", "bool", function(_, eof)
    if playing and not playing.settling and eof then
        if playing.link then finish(true) else at_end() end
    end
end)

-- ---------- the mouse and the key ----------

mp.observe_property("mouse-pos", "native", function(_, m)
    if not icon_rect or type(m) ~= "table" then return end
    -- (A player without a window, in tests, never has the mouse in it)
    local over = (m.hover ~= false or test_dims ~= nil) and m.x >= icon_rect.x0 and m.x < icon_rect.x1 and m.y >= icon_rect.y0 and m.y < icon_rect.y1
    if over == hovered then return end
    hovered = over
    if over then start("hover") else stop("hover") end
    draw()
end)

local key_bound = false
local function bind_key(on)
    if on == key_bound then return end
    key_bound = on
    if on then
        -- Space held: the live part; on a photo Space has nothing else to do (there's no playing to pause)
        mp.add_forced_key_binding("SPACE", "live-photo-hold", function(e)
            if e.event == "down" then start("key")
            elseif e.event == "up" then stop("key")
            elseif e.event == "press" then start("key") mp.add_timeout(0.15, function() stop("key") end) end
        end, { complex = true })
    else
        mp.remove_key_binding("live-photo-hold")
    end
end

mp.register_script_message("live-photo-view", function(what)
    if what == "play" then start("message") elseif what == "stop" then stop("message") end
end)

-- Tests: lay the icon out as on a window w x h at that HiDPI scale, with the picture's margins (left, top, right,
-- bottom) when given (a player without a window has no OSD to measure)
mp.register_script_message("live-photo-layout", function(w, h, s, ml, mt, mr, mb)
    test_dims = { w = tonumber(w) or 0, h = tonumber(h) or 0, scale = tonumber(s) or 1,
                  ml = tonumber(ml) or 0, mt = tonumber(mt) or 0, mr = tonumber(mr) or 0, mb = tonumber(mb) or 0 }
    draw()
end)

mp.register_script_message("live-photo-dump", function(path)
    local out = { photo = info and { path = info.path, live = info.live, kind = info.kind, duration = info.duration,
                                     still_t = info.still_t, start = info.start, url = info.url } or nil,
                  playing = playing and { step = playing.step, at = playing.run[playing.step].path,
                                          run = (function() local r = {} for _, d in ipairs(playing.run) do r[#r + 1] = d.path end return r end)() } or nil,
                  icon = icon_rect, image_vid = image_vid, hovered = hovered, hdr_vid = hdr.vid, hdr_on = hdr.target ~= nil,
                  ass = overlay.data, res = { overlay.res_x, overlay.res_y } }
    if info and info.live then
        local chain = {}
        for _, d in ipairs(live.chain(info, around(info.path))) do chain[#chain + 1] = { path = d.path, start = d.start, duration = d.duration } end
        out.chain = chain
    end
    local f = io.open(path, "w")
    if f then f:write(utils.format_json(out)) f:close() end
end)

-- ---------- loading ----------

-- Before the other plugins' (workspace 10, gyroflow 50, view360 60): they ask user-data/afp/photo
mp.add_hook("on_load", 5, function()
    local path = mp.get_property("path")
    if playing then playing = nil end
    info, image_vid = nil, nil
    if path and (live.is_photo(path) or live.is_link(path)) then
        described[path] = nil -- read again: it may have changed since
        info = describe(path)
    end
    if info and info.kind == "link" then
        -- Its run (the links of its clip after it in the playlist, their parts overlapping): the part the workspace
        -- plugin opens (user-data/afp/photo's span), the clip's time from its first start to its last end
        info.run = live.chain(info, around(path))
        info.span = { live.span(info.run) }
        publish()
        mp.set_property("file-local-options/keep-open", "always")
        return
    end
    publish()
    if info then
        -- The picture stays (no slideshow); the live part's end doesn't end the file
        mp.set_property("file-local-options/image-display-duration", "inf")
        mp.set_property("file-local-options/keep-open", "always")
        mp.set_property("file-local-options/pause", "yes")
    end
end)

-- A Live Photo's .mov out of the playlist when its photo is in it: a few at a time, each checked as a Live Photo's
-- video (short, or with Apple's keys) so a camera's clip beside its own JPEG stays
local pruning = nil
local function prune()
    if pruning then pruning:kill() end
    local list = playlist_paths()
    local photos = {}
    for _, p in ipairs(list) do
        if live.is_photo(p) then
            local stem = p:match("^(.*)%.[^./\\]+$")
            if stem then photos[stem:lower()] = p end
        end
    end
    local current = mp.get_property("path")
    local todo = {}
    for _, p in ipairs(list) do
        local stem, ext = p:match("^(.*)%.([^./\\]+)$")
        if ext and ext:lower() == "mov" and photos[stem:lower()] and p ~= current then todo[#todo + 1] = p end
    end
    if #todo == 0 then return end
    pruning = mp.add_periodic_timer(0.01, function()
        local p = table.remove(todo, 1)
        if not p then pruning:kill() pruning = nil return end
        local ok, mv = pcall(live.movie, p)
        if ok and mv and (mv.id or mv.still_t or mv.duration <= 6) then
            local i = index_of(p)
            if i then mp.commandv("playlist-remove", tostring(i - 1)) end
        end
    end)
end

mp.register_event("file-loaded", function()
    hovered = false
    hdr.vid, hdr.path, hdr.target = nil, nil, nil
    -- A link the workspace plugin didn't open as its clip's part (mpv's first load, before the folder's playlist;
    -- its clip not found): not live here
    if info and info.kind == "link" then
        local ws = mp.get_property_native("user-data/workspace")
        if not (type(ws) == "table" and type(ws.photo) == "table" and ws.photo.file == info.path) then info = nil publish() end
    end
    if info and info.kind ~= "link" then
        for _, tr in ipairs(tracks()) do
            if tr.type == "video" and tr.selected then image_vid = tr.id end
        end
    end
    bind_key(info ~= nil and info.live == true)
    draw()
    prune()
    if info and info.live then
        if info.kind ~= "link" then add_video(info) end
        warm(info.path)
    end
    if info then want_hdr(info) end
end)

mp.register_event("end-file", function()
    if playing then playing = nil end
    bind_key(false)
end)

mp.observe_property("osd-dimensions", "native", function() draw() end)
mp.observe_property("display-hidpi-scale", "number", function() draw() end)
