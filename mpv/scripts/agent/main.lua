-- agent: what's on the screen and what the player is doing, for an AI agent (Claude Code) working on this project
-- together with you. An mpv plugin: this folder goes into ~~/scripts/.
--
-- A note for the agent (Ctrl+r, or the menu's ActionFootagePlayer > Note for the agent…): the video pauses, you drag
-- a rectangle round what you mean (Shift+drag adds another, a plain drag starts again), Enter, a line of comment,
-- Enter. Esc in the comment goes back to the rectangles, Esc there leaves it. Each note is a folder of its own in
-- <data>/reports/, for the agent to read later:
--   report.json   the comment, the areas (in the frame's pixels, the window's, and 0..1 of the frame), where in the
--                 clip, and the state: the plugins' (user-data/) and the player's (what plays, how it's shown, the
--                 display)
--   screen.png    the window as it was, with what was over the video (telemetry, messages)
--   marked.png    the same with the areas drawn on it
--   frame.png     the frame at its own size, with nothing over it
--   log.txt       mpv's recent log (verbose)
--
-- Live, the agent reaches the player through mpv's JSON IPC (https://mpv.io/manual/master/#json-ipc): the socket's
-- path is in <data>/session.json (socket, pid) while the player runs. An input-ipc-server set elsewhere (mpv.conf, the
-- Windows app's pipe) is kept; otherwise it's <data>/mpv.sock (on Windows \\.\pipe\ActionFootagePlayer-agent; a path
-- too long for a socket: $TMPDIR/afp-agent-<user>.sock).
--
-- <data> is $AGENT_DATA (the app's: ~/Library/Application Support/ActionFootagePlayer/agent/,
-- %LOCALAPPDATA%\ActionFootagePlayer\agent\), else ~~/agent/.
--
-- Keys (rebind with `<key> script-binding agent/<name>` in input.conf):
--   Ctrl+r  report   a note for the agent
--
-- Script messages (`script-message-to agent <message>`, what the menu and the agent run):
--   report               a note for the agent, as Ctrl+r
--   snapshot [<file>]    the state as report.json has it, into <file> (default <data>/state.json)
--   say <text> [<sec>]   a message from the agent, over the video (default 6 seconds)
--
-- Options (~~/script-opts/agent.conf):
--   socket=   the IPC socket (or pipe) when mpv has none; empty: as above

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'
local input = require 'mp.input'

local o = { socket = "" }
options.read_options(o, "agent")

local windows = mp.get_property("platform") == "windows"
local DATA = os.getenv("AGENT_DATA") or mp.command_native({ "expand-path", "~~/agent" })
local REPORTS = utils.join_path(DATA, "reports")
-- report.json's layout, for the agent
local FORMAT = 1
local LOG_LINES = 2000
-- A drag shorter than this (of the frame's width or height) is a click, not an area
local MIN_AREA = 0.005
-- What report.json says of the player
local PLAYER = {
    "path", "filename", "working-directory", "duration", "time-pos", "estimated-frame-number", "estimated-frame-count",
    "pause", "speed", "container-fps", "estimated-vf-fps", "playlist-pos", "playlist-count", "ab-loop-a", "ab-loop-b",
    "video-params", "video-out-params", "hwdec-current", "vf", "glsl-shaders", "current-vo", "current-gpu-context",
    "osd-dimensions", "fullscreen", "window-scale", "video-zoom", "video-pan-x", "video-pan-y", "display-names",
    "display-fps", "display-hidpi-scale", "target-colorspace-hint", "target-prim", "target-trc", "target-peak",
    "tone-mapping", "frame-drop-count", "decoder-frame-drop-count", "vo-delayed-frame-count", "mpv-version", "platform",
    "input-ipc-server",
}

local function mkdir(dir)
    if utils.file_info(dir) then return true end
    local r = mp.command_native({ name = "subprocess", playback_only = false,
                                  args = windows and { "cmd", "/d", "/c", "mkdir", dir } or { "mkdir", "-p", dir } })
    return r and r.status == 0
end

local function write(path, text)
    local f = io.open(path, "wb")
    if not f then return false end
    f:write(text)
    f:close()
    return true
end

-- mpv's log, the last LOG_LINES lines, for log.txt

local log, log_next = {}, 1
mp.enable_messages("v")
mp.register_event("log-message", function(e)
    log[log_next] = string.format("%10.3f [%s] %s: %s", mp.get_time(), e.level, e.prefix, (e.text:gsub("\n$", "")))
    log_next = log_next % LOG_LINES + 1
end)

local function recent_log()
    local lines = {}
    for i = 0, LOG_LINES - 1 do
        local line = log[(log_next - 1 + i) % LOG_LINES + 1]
        if line then table.insert(lines, line) end
    end
    return table.concat(lines, "\n") .. "\n"
end

local function state()
    local player = {}
    for _, name in ipairs(PLAYER) do player[name] = mp.get_property_native(name) end
    return { plugins = mp.get_property_native("user-data") or {}, player = player }
end

-- The IPC socket, and where the agent finds it

mkdir(DATA)
local socket = mp.get_property("input-ipc-server", "")
if socket == "" then
    socket = o.socket ~= "" and mp.command_native({ "expand-path", o.socket })
             or (windows and [[\\.\pipe\ActionFootagePlayer-agent]] or utils.join_path(DATA, "mpv.sock"))
    -- A Unix socket's path has room for 104 bytes on macOS: a longer one, in the user's temporary folder
    if not windows and #socket > 100 then
        socket = utils.join_path(os.getenv("TMPDIR") or "/tmp", "afp-agent-" .. (os.getenv("USER") or "") .. ".sock")
    end
    mp.set_property("input-ipc-server", socket)
end
local SESSION = utils.join_path(DATA, "session.json")
local pid = mp.get_property_number("pid")
write(SESSION, utils.format_json({ socket = socket, pid = pid, mpv = mp.get_property("mpv-version"),
                                   started = os.date("!%Y-%m-%dT%H:%M:%SZ") }))
-- For the menu, and the agent: where things are
mp.set_property_native("user-data/agent", { socket = socket, reports = REPORTS })
mp.register_event("shutdown", function()
    -- Another player started since has it now
    local f = io.open(SESSION, "rb")
    local session = f and utils.parse_json(f:read("*a"))
    if f then f:close() end
    if session and session.pid == pid then os.remove(SESSION) end
end)

-- The note for the agent

local overlay = mp.create_osd_overlay("ass-events")
-- The one being made: dir, areas ({ x0, y0, x1, y1 }, 0..1 of the frame), drag (the one being drawn), paused and
-- dragging and osc (its visibility; as they were), asking (the comment's being written), writing (images still being written)
local report = nil

-- The video's rectangle in the window (OSD coordinates, as mouse-pos)
local function video_rect()
    local d = mp.get_property_native("osd-dimensions")
    if not d or d.w <= 0 or d.h <= 0 then return nil end
    return d, d.ml, d.mt, d.w - d.mr, d.h - d.mb
end

local function to_frame(x, y)
    local _, x0, y0, x1, y1 = video_rect()
    if not x0 or x1 <= x0 or y1 <= y0 then return 0, 0 end
    local clamp = function(v) return math.max(0, math.min(1, v)) end
    return clamp((x - x0) / (x1 - x0)), clamp((y - y0) / (y1 - y0))
end

local function from_frame(u, v)
    local _, x0, y0, x1, y1 = video_rect()
    return x0 + u * (x1 - x0), y0 + v * (y1 - y0)
end

-- A drag's corners, either way it went
local function normalized(a)
    return { math.min(a[1], a[3]), math.min(a[2], a[4]), math.max(a[1], a[3]), math.max(a[2], a[4]) }
end

local function draw(hint)
    local d = video_rect()
    if not report or not d then overlay:remove() return end
    overlay.res_x, overlay.res_y = d.w, d.h
    local size, events = d.h / 36, {}
    local areas = {}
    for i, a in ipairs(report.areas) do areas[i] = a end
    if report.drag then table.insert(areas, normalized(report.drag)) end
    for i, a in ipairs(areas) do
        local x0, y0 = from_frame(a[1], a[2])
        local x1, y1 = from_frame(a[3], a[4])
        table.insert(events, string.format("{\\an7\\pos(0,0)\\bord%g\\shad0\\1a&HFF&\\3c&H00D7FF&\\p1}m %d %d l %d %d l %d %d l %d %d{\\p0}",
                                           size / 9, x0, y0, x1, y0, x1, y1, x0, y1))
        table.insert(events, string.format("{\\an1\\pos(%d,%d)\\fs%g\\b1\\bord%g\\shad0\\1c&H00D7FF&\\3c&H000000&}%d",
                                           x0 + size / 6, y0 - size / 6, size, size / 12, i))
    end
    if hint then
        table.insert(events, string.format("{\\an8\\pos(%d,%d)\\fs%g\\bord%g\\shad0\\4a&H60&}%s", d.w / 2, size, size * 0.8, size / 16,
            "Note for the agent: drag round what you mean (Shift+drag adds one)\\NEnter: comment    Esc: leave"))
    end
    overlay.data = table.concat(events, "\n")
    overlay:update()
end

-- Notes left while their images were still being written: dir → images still being written
local left = {}

local function remove_dir(dir)
    if windows then
        mp.command_native({ name = "subprocess", playback_only = false, args = { "cmd", "/d", "/c", "rmdir", dir } })
    else
        os.remove(dir)
    end
end

-- An image of the window or the frame, into the note's folder. The image is taken at once, in the order of the
-- commands (what's drawn after it isn't in it), and written in the background: a 4K PNG takes seconds
local function shot(name, mode)
    local dir = report.dir
    report.writing = (report.writing or 0) + 1
    mp.command_native_async({ "screenshot-to-file", utils.join_path(dir, name), mode }, function(ok, _, err)
        if not ok then mp.msg.warn("couldn't save " .. name .. ": " .. tostring(err)) end
        if left[dir] then
            os.remove(utils.join_path(dir, name))
            left[dir] = left[dir] - 1
            if left[dir] == 0 then left[dir] = nil; remove_dir(dir) end
            return
        end
        if report and report.dir == dir then report.writing = report.writing - 1 end
    end)
end

local BUTTONS = { "MBTN_LEFT", "Shift+MBTN_LEFT", "Ctrl+MBTN_LEFT", "Meta+MBTN_LEFT" }
local unbind, ask, leave

local function mouse_moved(_, pos)
    if not report or not report.drag or not pos then return end
    report.drag[3], report.drag[4] = to_frame(pos.x, pos.y)
    draw(true)
end

local function button(add)
    return function(e)
        if not report then return end
        local pos = mp.get_property_native("mouse-pos")
        if e.event == "down" and pos then
            if not add then report.areas = {} end
            local u, v = to_frame(pos.x, pos.y)
            report.drag = { u, v, u, v }
            mp.observe_property("mouse-pos", "native", mouse_moved)
        elseif e.event == "up" and report.drag then
            mp.unobserve_property(mouse_moved)
            if pos then report.drag[3], report.drag[4] = to_frame(pos.x, pos.y) end
            local a = normalized(report.drag)
            if a[3] - a[1] >= MIN_AREA and a[4] - a[2] >= MIN_AREA then table.insert(report.areas, a) end
            report.drag = nil
            draw(true)
        end
    end
end

local function bind()
    for i, key in ipairs(BUTTONS) do
        mp.add_forced_key_binding(key, "report-drag-" .. i, button(key ~= "MBTN_LEFT"), { complex = true })
    end
    mp.add_forced_key_binding("MBTN_LEFT_DBL", "report-double", function() end)
    mp.add_forced_key_binding("ENTER", "report-comment", function() ask() end)
    mp.add_forced_key_binding("KP_ENTER", "report-comment-kp", function() ask() end)
    mp.add_forced_key_binding("ESC", "report-leave", function() leave() end)
    -- These keys alone, and no OSC: shown by the mouse, it would take Shift+click
    mp.input_enable_section("input_forced_" .. mp.get_script_name(), "exclusive")
    if report.osc then mp.commandv("script-message-to", "osc", "osc-visibility", "never", "no_osd") end
end

unbind = function()
    mp.unobserve_property(mouse_moved)
    if report.osc then mp.commandv("script-message-to", "osc", "osc-visibility", report.osc, "no_osd") end
    for i in ipairs(BUTTONS) do mp.remove_key_binding("report-drag-" .. i) end
    for _, name in ipairs({ "report-double", "report-comment", "report-comment-kp", "report-leave" }) do
        mp.remove_key_binding(name)
    end
end

local function finish()
    unbind()
    if report.asking then input.terminate() end
    overlay:remove()
    mp.set_property_native("window-dragging", report.dragging)
    if not report.paused then mp.set_property_native("pause", false) end
    report = nil
end

leave = function()
    if not report then return end
    local dir, writing = report.dir, report.writing or 0
    finish()
    for _, name in ipairs({ "screen.png", "frame.png", "marked.png" }) do os.remove(utils.join_path(dir, name)) end
    if writing > 0 then left[dir] = writing else remove_dir(dir) end
end

local function save(comment)
    local d = mp.get_property_native("osd-dimensions") or {}
    local out = mp.get_property_native("video-out-params") or {}
    local fw, fh = out.dw or out.w or 0, out.dh or out.h or 0
    local areas = {}
    for i, a in ipairs(report.areas) do
        local x0, y0 = from_frame(a[1], a[2])
        local x1, y1 = from_frame(a[3], a[4])
        local round = function(v) return math.floor(v + 0.5) end
        areas[i] = {
            n = i,
            relative = { x = a[1], y = a[2], w = a[3] - a[1], h = a[4] - a[2] },
            frame = { x = round(a[1] * fw), y = round(a[2] * fh), w = round((a[3] - a[1]) * fw), h = round((a[4] - a[2]) * fh) },
            screen = { x = round(x0), y = round(y0), w = round(x1 - x0), h = round(y1 - y0) },
        }
    end
    local json = utils.format_json({
        format = FORMAT, comment = comment, created = report.created,
        clip = mp.get_property("path"), time = report.time, frame_number = report.frame_number,
        frame_size = { w = fw, h = fh }, screen_size = { w = d.w, h = d.h },
        areas = areas, files = { screen = "screen.png", marked = "marked.png", frame = "frame.png", log = "log.txt" },
        state = report.state,
    })
    local dir = report.dir
    local saved = write(utils.join_path(dir, "report.json"), json) and write(utils.join_path(dir, "log.txt"), recent_log())
    finish()
    mp.osd_message(saved and ("Saved for the agent: " .. dir) or ("Couldn't save the note in " .. dir), saved and 4 or 8)
end

-- marked.png, the areas without the hint, then the comment
ask = function()
    if not report or report.drag then return end
    unbind()
    report.asking = true
    draw(false)
    -- Taken before the comment's line is drawn over it (see shot)
    shot("marked.png", "window")
    local submitted = false
    input.get({
        prompt = "Comment for the agent: ",
        submit = function(text)
            submitted = true
            report.asking = false
            input.terminate()
            save(text)
        end,
        -- Esc: back to the areas
        closed = function()
            if submitted or not report then return end
            report.asking = false
            bind()
            draw(true)
        end,
    })
end

local function start()
    if report then return end
    local path = mp.get_property("path")
    if not path or not mp.get_property_native("video-out-params") then
        mp.osd_message("A note for the agent needs a video playing", 3)
        return
    end
    -- First of all, the frame you saw stays
    local paused = mp.get_property_native("pause")
    mp.set_property_native("pause", true)
    local stem = (mp.get_property("filename/no-ext") or "clip"):gsub("[^%w%-_]", "_"):sub(1, 40)
    local dir = utils.join_path(REPORTS, os.date("%Y-%m-%d_%H%M%S") .. "_" .. stem)
    if not mkdir(dir) then
        mp.set_property_native("pause", paused)
        mp.osd_message("Couldn't make " .. dir, 6)
        return
    end
    report = {
        -- As it is, before the note hides the OSC (and as it was before it paused the video)
        state = state(),
        dir = dir, areas = {}, created = os.date("%Y-%m-%dT%H:%M:%S"),
        time = mp.get_property_number("time-pos"), frame_number = mp.get_property_number("estimated-frame-number"),
        paused = paused, dragging = mp.get_property_native("window-dragging"),
        osc = mp.get_property_native("user-data/osc/visibility"),
    }
    report.state.player.pause = paused
    mp.set_property_native("window-dragging", false)
    -- The window as it is, before anything of the note's is over it
    shot("screen.png", "window")
    shot("frame.png", "video")
    bind()
    draw(true)
end

mp.add_key_binding("Ctrl+r", "report", start)
mp.register_script_message("report", start)
mp.observe_property("osd-dimensions", "native", function() if report then draw(not report.asking) end end)
-- Another clip: the note was about this one
mp.register_event("start-file", function() if report then leave() end end)

mp.register_script_message("snapshot", function(file)
    file = file and file ~= "" and file or utils.join_path(DATA, "state.json")
    if not write(file, utils.format_json(state())) then mp.msg.warn("couldn't write " .. file) end
end)

mp.register_script_message("say", function(text, seconds)
    mp.osd_message(text or "", tonumber(seconds) or 6)
end)
