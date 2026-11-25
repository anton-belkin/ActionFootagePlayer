-- The picture over the seek bar under the mouse (docs/design/seek-and-scrub.md, "Preview on hover"): on plain hover and while
-- the knob is dragged, the frame at the mouse's time, from the camera's low-resolution copy beside the clip when it has
-- one (DJI .LRF, GoPro .LRV: the exact frame), else the clip's keyframe at or before it. As the file has it: no LUT,
-- no stabilization. Made by afp-scrub (tools/scrub/afp-scrub.c), a helper that keeps the files and their decoders
-- open, when the mouse asks: nothing is made ahead, nothing written beside the clips; the pictures made are kept in
-- the helper's memory while the player runs. Shown with mpv's overlay-add (raw BGRA from a file).
--
-- user-data/afp-scrub/shown: the picture on screen, { id, t (its frame's time in the clip), kind (exact, walk, key,
-- cache), ms (asked -> on screen) }, absent for none (for tests and the agent).
--
-- The helper reads its requests from the property user-data/afp-scrub/request and answers with
-- `script-message-to side_panel scrub <reply>`, over mpv's IPC socket (the agent plugin's, else one of its own).
-- Insta360 and other 360 clips: no picture (postponed).
--
-- The plain part (names, sizes, places, the replies) needs no mpv: tests/scrub_test.lua.

local M = {}

-- The picture's height (points): about twice the bar's
M.HEIGHT = 130
-- Above the bar's time bubble (bar.lua: 30 points above the bar's top), this far apart
M.GAP = 6
-- overlay-add's id (0-63, the player's own; nothing else in it uses one)
M.OVERLAY = 37
-- Times nearer than this to the one asked last aren't asked again (s): the clip's frames at 60 fps
M.QUANTUM = 1 / 120

local function dir_of(path) return path:match("^(.*[/\\])") or "" end
local function name_of(path) return path:match("([^/\\]+)$") or path end

-- The names a clip's camera-made low-resolution copy may have beside it, in the order looked for. The Library's rule
-- (library/src/index.rs, low_res_names()); tests/scrub_sidecar_cases.txt holds both to it
function M.sidecar_names(clip)
    local name = name_of(clip)
    local stem = name:match("^(.*)%.[^.]+$") or name
    local names = { stem .. ".LRF", stem .. ".lrf" }
    if #stem > 2 and (stem:sub(1, 2) == "GX" or stem:sub(1, 2) == "GH") then
        names[#names + 1] = "GL" .. stem:sub(3) .. ".LRV"
        names[#names + 1] = "GL" .. stem:sub(3) .. ".lrv"
    end
    if stem:sub(1, 4) == "GOPR" or (#stem > 2 and stem:sub(1, 2) == "GP") then
        names[#names + 1] = stem .. ".LRV"
        names[#names + 1] = stem .. ".lrv"
    end
    return names
end

-- The sidecar beside the clip: is_file(path) -> size or nil. One cut short (the camera stopped before writing it)
-- isn't one
function M.find_sidecar(clip, size_of)
    local dir = dir_of(clip)
    for _, n in ipairs(M.sidecar_names(clip)) do
        local size = size_of(dir .. n)
        if size and size >= 1024 then return dir .. n end
    end
    return nil
end

-- What the picture is made from for the file playing, or nil (and why) for none:
--   path       mpv's path
--   ws         user-data/workspace (a moment's link plays its original's part: clip, offset)
--   is360      the clip is a sphere (view360 says so)
-- { clip (the file whose time it is), offset (the clip's time = the time playing + offset) }
function M.source(path, ws, is360)
    if type(path) ~= "string" or path == "" then return nil, "nothing playing" end
    if is360 then return nil, "360" end
    local clip, offset = path, 0
    if type(ws) == "table" and ws.path == path and type(ws.clip) == "string" then
        clip = ws.clip
        offset = tonumber(ws.offset) or 0
    end
    if clip:match("^%a[%w+.-]*://") and not clip:match("^file://") then return nil, "not a file" end
    clip = clip:gsub("^file://", "")
    local ext = (clip:match("%.([^./\\]+)$") or ""):lower()
    if ext == "insv" or ext == "360" then return nil, "360" end
    return { clip = clip, offset = offset }
end

-- The picture's size in pixels for a video of aspect (width/height), s pixels a point: HEIGHT points high, as wide as
-- the aspect has it (even numbers)
function M.size(aspect, s)
    if type(aspect) ~= "number" or aspect <= 0 then aspect = 16 / 9 end
    aspect = math.max(0.4, math.min(aspect, 3))
    local h = math.floor(M.HEIGHT * s / 2 + 0.5) * 2
    local w = math.floor(h * aspect / 2 + 0.5) * 2
    return w, h
end

-- Where it goes (its top left, pixels): centred on the mouse's x (mx), kept within x0..x1, above the time bubble over
-- the bar whose top is at top
function M.place(mx, w, h, x0, x1, top, s)
    local x = mx - w / 2
    x = math.max(x0 + 4 * s, math.min(x, x1 - w - 4 * s))
    local y = top - 30 * s - M.GAP * s - h
    return math.floor(x + 0.5), math.floor(y + 0.5)
end

-- A request's line for the helper: id, file, time, mode (exact, key; open-exact, open-key: the file opened, nothing
-- made), out, w, h[, clip]
function M.request(id, file, t, mode, out, w, h, clip)
    local f = { tostring(id), file, string.format("%.4f", math.max(0, t)), mode, out, tostring(w), tostring(h) }
    if clip then f[#f + 1] = clip end
    return table.concat(f, "\t")
end

-- The helper's reply: { id, ok, ms, kind, t } or { id, status (dropped, error), message }
function M.parse_reply(line)
    if type(line) ~= "string" then return nil end
    local f = {}
    for part in (line .. "\t"):gmatch("([^\t]*)\t") do f[#f + 1] = part end
    local id = tonumber(f[1])
    if not id then return nil end
    if f[2] == "ok" then return { id = id, ok = true, ms = tonumber(f[3]), kind = f[4], t = tonumber(f[5]) } end
    return { id = id, ok = false, status = f[2], message = f[3] }
end

-- Times taken (ms): median, p90, max
function M.summary(list)
    if #list == 0 then return nil end
    local v = {}
    for i, x in ipairs(list) do v[i] = x end
    table.sort(v)
    local function q(f) return v[math.min(#v, math.floor(#v * f) + 1)] end
    return { n = #v, median = q(0.5), p90 = q(0.9), max = v[#v] }
end

------------------------------------------------------------------------------------------------------------------------
-- In the player: M.new(mp, utils, J, platform) -> the preview, with
--   want(t, mx, x0, x1, top, s)   the mouse is at time t (the file playing's), x mx over the track: the picture for it
--                                 asked for; returns its place { x, y, w, h } while one is shown, for the bar's frame
--   warm()                        the mouse is over the bar: the helper started, ready for the track
--   hide()                        the mouse left the track
--   reply(line)                   the helper's answer (script message "scrub")

function M.new(mp, utils, J, platform)
    local P = {}
    local windows = platform == "windows"
    if type(utils.join_path) ~= "function" or type(utils.file_info) ~= "function" then return { available = false,
        want = function() end, warm = function() end, hide = function() end, reply = function() end, shown = function() end,
        forget_clip = function() end, quit = function() end } end
    local exe = (function()
        local dir = mp.get_script_directory and mp.get_script_directory()
        local plugins = dir and dir:match("^(.*)[/\\][^/\\]+[/\\]?$")
        if not plugins then return nil end
        for _, d in ipairs({ dir, utils.join_path(plugins, "gyroflow") }) do
            local p = utils.join_path(d, "afp-scrub" .. (windows and ".exe" or ""))
            if utils.file_info(p) then return p end
        end
    end)()
    local pid = mp.get_property_number("pid") or 0
    local tmp = utils.join_path(os.getenv(windows and "TEMP" or "TMPDIR") or "/tmp", "afp-scrub-" .. pid)
    local made_tmp = false
    local running, failures, socket, quitting = false, 0, nil, false
    local seq = 0
    -- The last asked: { id, key (file + time), w, h, out, at, mx, x0, x1, top, s }; the sizes of the last few asked
    local asked = nil
    local sizes = {}
    -- The one on screen: { id, x, y, w, h }
    local shown = nil
    -- Per file playing: { path, src (M.source), sidecar, why }
    local clip = nil
    -- This hover: its journal action, when it started, the times taken
    local hover = nil

    local function log(...) mp.msg.verbose("scrub: " .. string.format(...)) end

    local function ensure_socket()
        local s = mp.get_property("input-ipc-server", "")
        if s ~= "" then return s end
        s = windows and ([[\\.\pipe\afp-scrub-]] .. pid) or utils.join_path(os.getenv("TMPDIR") or "/tmp", "afp-scrub-" .. pid .. ".sock")
        if not windows and #s > 100 then s = "/tmp/afp-scrub-" .. pid .. ".sock" end
        mp.set_property("input-ipc-server", s)
        return s
    end

    local function start()
        if running then return true end
        if not exe or failures > 3 or windows then return false end
        if not made_tmp then
            mp.command_native({ name = "subprocess", playback_only = false, args = { "mkdir", "-p", tmp } })
            made_tmp = true
        end
        socket = ensure_socket()
        running = true
        log("starting %s on %s", exe, socket)
        mp.command_native_async({ name = "subprocess", playback_only = false, capture_stderr = true,
                                  args = { exe, "--ipc", socket, "--pid", tostring(pid), "--to", mp.get_script_name() } },
            function(_, r)
                running = false
                if quitting then return end
                -- Started again at the next hover, three times at most
                failures = failures + 1
                mp.msg.warn("afp-scrub ended: " .. tostring(r and r.status) .. " " .. tostring(r and r.stderr or ""))
            end)
        return true
    end

    -- The file playing's source, looked up once per file (file_info of a few names: on a NAS a few ms each)
    local function source()
        local path = mp.get_property("path")
        if clip and clip.path == path then return clip end
        local v360 = mp.get_property_native("user-data/view360")
        local src, why = M.source(path, mp.get_property_native("user-data/workspace"), type(v360) == "table" and v360.available == true)
        clip = { path = path, src = src, why = why }
        if src then
            clip.sidecar = M.find_sidecar(src.clip, function(p) local i = utils.file_info(p); return i and i.is_file and i.size or nil end)
            local vp = mp.get_property_native("video-params") or {}
            local aspect = vp.aspect or ((vp.dw or 0) > 0 and (vp.dh or 0) > 0 and vp.dw / vp.dh) or nil
            local rot = tonumber(vp.rotate) or 0
            if aspect and (rot == 90 or rot == 270) then aspect = 1 / aspect end
            clip.aspect = aspect
        end
        log("%s: %s", tostring(path), src and (clip.sidecar and ("sidecar " .. clip.sidecar) or "keyframes") or tostring(why))
        return clip
    end

    local function remove_overlay()
        if shown then
            mp.command_native({ "overlay-remove", M.OVERLAY })
            mp.set_property_native("user-data/afp-scrub/shown", nil)
        end
        shown = nil
    end

    function P.hide()
        remove_overlay()
        if hover then
            local s = M.summary(hover.ms)
            if s then
                J.step("bar/preview-pictures", { pictures = s.n, median = s.median, p90 = s.p90, max = s.max, kinds = hover.kinds,
                                                 sidecar = hover.sidecar })
            end
            hover = nil
        end
        asked = nil
    end

    function P.want(t, mx, x0, x1, top, s)
        if type(t) ~= "number" then return nil end
        local c = source()
        if not c.src then return nil end
        local w, h = M.size(c.aspect, s)
        if asked then asked.mx, asked.x0, asked.x1, asked.top, asked.s = mx, x0, x1, top, s end
        -- The picture shown follows the mouse at once; a new one is asked for its time
        if shown then
            local x, y = M.place(mx, shown.w, shown.h, x0, x1, top, s)
            if x ~= shown.x or y ~= shown.y then
                shown.x, shown.y = x, y
                if shown.out_kept then
                    mp.command_native({ "overlay-add", M.OVERLAY, x, y, shown.out_kept, 0, "bgra", shown.w, shown.h, shown.w * 4 })
                end
            end
        end
        local ct = t + c.src.offset
        local exact = c.sidecar ~= nil
        local key = string.format("%s|%d|%dx%d", c.sidecar or c.src.clip, math.floor(ct / M.QUANTUM + 0.5), w, h)
        if asked and asked.key == key then return shown end
        if not start() then return shown end
        if not hover then
            hover = { a = J.act("bar/preview", { sidecar = exact }), ms = {}, kinds = {}, sidecar = exact }
        end
        seq = seq + 1
        local out = utils.join_path(tmp, seq .. ".bgra")
        asked = { id = seq, key = key, w = w, h = h, out = out, at = mp.get_time(), mx = mx, x0 = x0, x1 = x1, top = top, s = s }
        sizes[seq] = { w, h, asked.at }
        sizes[seq - 16] = nil
        mp.set_property_native("user-data/afp-scrub/request",
                        M.request(seq, exact and c.sidecar or c.src.clip, ct, exact and "exact" or "key", out, w, h, exact and c.src.clip or nil))
        return shown
    end

    -- The helper's answer: a picture newer than the one shown goes on screen (the newest asked may still be on its
    -- way: this one is nearer than the one there), centred on where the mouse is now
    function P.reply(line)
        local r = M.parse_reply(line)
        if not r then return end
        local a = asked
        if not r.ok then
            if r.status == "error" then log("%s", line) end
            return
        end
        local out = utils.join_path(tmp, r.id .. ".bgra")
        local size = sizes[r.id]
        if not a or not size or r.id > a.id or (shown and r.id <= shown.id) then
            os.remove(out)
            return
        end
        local w, h = size[1], size[2]
        local x, y = M.place(a.mx, w, h, a.x0, a.x1, a.top, a.s)
        mp.command_native({ "overlay-add", M.OVERLAY, x, y, out, 0, "bgra", w, h, w * 4 })
        -- The last one kept on disk (it's moved as the mouse moves), the one before removed
        if shown and shown.out_kept and shown.out_kept ~= out then os.remove(shown.out_kept) end
        shown = { id = r.id, x = x, y = y, w = w, h = h, out_kept = out }
        local ms = (mp.get_time() - size[3]) * 1000
        mp.set_property_native("user-data/afp-scrub/shown", { id = r.id, t = r.t, kind = r.kind, ms = ms, helper_ms = r.ms, x = x, y = y, w = w, h = h })
        if hover then
            if r.id == a.id then hover.ms[#hover.ms + 1] = ms end
            hover.kinds[r.kind or "?"] = (hover.kinds[r.kind or "?"] or 0) + 1
            if not hover.first then
                hover.first = true
                J.shown_for(hover.a, "preview", { ms = ms, kind = r.kind, helper_ms = r.ms })
            end
        end
        return shown
    end

    -- The mouse came over the bar: the helper started (a process, nothing read), so the first picture doesn't wait
    -- for it
    function P.warm()
        local c = source()
        if not c.src or c.opened or not start() then return end
        c.opened = true
        seq = seq + 1
        local exact = c.sidecar ~= nil
        mp.set_property_native("user-data/afp-scrub/request", M.request(seq, exact and c.sidecar or c.src.clip, 0,
                               exact and "open-exact" or "open-key", "-", 1, 1, exact and c.src.clip or nil))
    end

    function P.shown() return shown end

    function P.forget_clip()
        P.hide()
        clip = nil
    end

    function P.quit()
        quitting = true
        remove_overlay()
        if made_tmp then
            for _, f in ipairs(utils.readdir(tmp, "files") or {}) do os.remove(utils.join_path(tmp, f)) end
            os.remove(tmp)
        end
    end

    P.available = exe ~= nil and not windows
    return P
end

return M
