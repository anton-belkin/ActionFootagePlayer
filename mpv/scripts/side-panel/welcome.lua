-- The welcome card: what the window shows when the player is open with no video (started from the Dock, the Start menu
-- or Launchpad, or a video closed), in place of an empty black window. How to begin, in a few steps: open a clip from
-- the camera, try the sample dive that comes with the app, set the camera for the next dive, tell us how it went.
-- Drawn in ASS in the panel's colors (ui.lua), on an overlay of its own under the cards (notify.lua).
--
-- It shows once the player has been idle for SHOW_AFTER seconds (opened from Finder, the video's Apple Event comes a
-- moment after the app starts: no flash of the card then), and goes as a file starts.
--
-- The sample: `sample/<SAMPLE>` beside the plugins (the Mac app's Contents/Resources/sample/, the Windows install's
-- sample\), put there by the packagers when the release has one. "Show the sample" copies it to the user's videos
-- folder (Movies/Action Footage Player on a Mac, Videos\Action Footage Player on Windows: the app's own folder is
-- read-only on a Mac, and the user should see where it is) and shows it there in Finder or Explorer. Without it the
-- step isn't shown.
--
-- Script messages (`script-message-to side_panel <message>`):
--   welcome [show|hide]   show or hide it now (the picture rig, the tests)

local mp = require 'mp'
local utils = require 'mp.utils'
local ui = require 'ui'

local M = {}
local C = ui.C
local SHOW_AFTER = 1.2
M.SAMPLE = "Sample dive - Osmo Action 6.mp4"
M.SITE = "https://actionfootageplayer.ch"
-- The key that shows a clip as recorded, while it's held (nil until the player has one: the line isn't shown)
M.COMPARE_KEY = nil
local TITLE_BAR = 28

local platform = mp.get_property("platform") or ""
local overlay = mp.create_osd_overlay("ass-events")
overlay.z = 15

local shown = false
local hits = {}
local hover = nil
local captured = false
local timer = nil

local function scale() return mp.get_property_number("display-hidpi-scale", 1) or 1 end

-- This plugin's folder's parent's parent: where `sample/` is in the app
local function app_dir()
    local dir = mp.get_script_directory and mp.get_script_directory() or nil
    if not dir then return nil end
    dir = dir:gsub("[/\\]+$", "")
    local parent = dir:match("^(.*)[/\\][^/\\]+$")
    return parent and parent:match("^(.*)[/\\][^/\\]+$") or nil
end

function M.sample_path()
    local base = app_dir()
    if not base then return nil end
    local path = utils.join_path(utils.join_path(base, "sample"), M.SAMPLE)
    local f = io.open(path, "rb")
    if not f then return nil end
    f:close()
    return path
end

-- Where the sample is copied to: the user's videos folder
function M.videos_dir()
    if platform == "windows" then
        local home = os.getenv("USERPROFILE")
        return home and (home .. "\\Videos\\Action Footage Player") or nil
    end
    local home = os.getenv("HOME")
    if not home then return nil end
    return home .. (platform == "darwin" and "/Movies" or "/Videos") .. "/Action Footage Player"
end

local function run(args)
    mp.command_native({ name = "subprocess", args = args, playback_only = false, capture_stdout = true, capture_stderr = true })
end

local function size_of(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local n = f:seek("end")
    f:close()
    return n
end

-- The sample copied (unless it's there already, whole) and shown in Finder or Explorer
function M.show_sample()
    local from, dir = M.sample_path(), M.videos_dir()
    if not from or not dir then return false end
    local to = utils.join_path(dir, M.SAMPLE)
    if size_of(to) ~= size_of(from) then
        if platform == "windows" then run({ "cmd", "/c", "mkdir", dir }) else run({ "mkdir", "-p", dir }) end
        local src, dst = io.open(from, "rb"), io.open(to, "wb")
        if not src or not dst then
            if src then src:close() end
            if dst then dst:close() end
            mp.osd_message("Couldn't copy the sample to " .. dir, 5)
            return false
        end
        while true do
            local chunk = src:read(4 * 1024 * 1024)
            if not chunk then break end
            dst:write(chunk)
        end
        src:close()
        dst:close()
    end
    if platform == "darwin" then run({ "open", "-R", to })
    elseif platform == "windows" then run({ "explorer", "/select," .. to })
    else run({ "xdg-open", dir }) end
    return true
end

local function open_url(url)
    if platform == "darwin" then run({ "open", url })
    elseif platform == "windows" then run({ "rundll32", "url.dll,FileProtocolHandler", url })
    else run({ "xdg-open", url }) end
end

-- The steps, as they're shown now: { title, detail, button = { id, label, cmd } }
function M.steps()
    local files = platform == "windows" and "File Explorer" or (platform == "darwin" and "Finder" or "your file manager")
    local steps = {
        { title = "Open a clip from your camera",
          detail = "Double-click it in " .. files .. ", or drop it on this window. Copy clips straight from the card: "
                   .. "apps that convert or trim them drop the motion data it steadies them with." },
    }
    if M.sample_path() then
        local detail = "An Osmo Action 6 dive as the camera recorded it: grey, wobbly and noisy. Double-click it, and "
                       .. "it plays here in color, steady and clean."
        if M.COMPARE_KEY then detail = detail .. " Hold " .. M.COMPARE_KEY .. " while it plays to see it as recorded." end
        steps[#steps + 1] = { title = "No clip at hand? Try the sample dive", detail = detail,
                              button = { id = "sample", label = "Show the sample", cmd = M.show_sample } }
    end
    steps[#steps + 1] = { title = "Before your next dive",
        detail = "Set the camera to log (D-Log M on DJI, GP-Log on GoPro) and turn its stabilization off; on DJI keep "
                 .. "the Wide field of view. Every dive after plays steady and in color.",
        button = { id = "how", label = "How", cmd = function() open_url(M.SITE .. "/divers.html#setup") end } }
    steps[#steps + 1] = { title = "Tell us how it went",
        detail = "What worked, what didn't, where you got stuck: a few words help more than you'd think.",
        button = { id = "feedback", label = "Send feedback", cmd = function() mp.commandv("script-message", "afp-feedback") end } }
    return steps
end

-- The video's place: beside the panel, above the bar when it's docked, under the title bar (macOS)
local function area(w, h)
    local right = mp.get_property_number("video-margin-ratio-right", 0) or 0
    local bottom = mp.get_property_number("video-margin-ratio-bottom", 0) or 0
    local s = scale()
    local t0 = (platform == "darwin" and not mp.get_property_native("fullscreen") and mp.get_property_native("border") ~= false)
               and TITLE_BAR * s or 0
    return { x0 = 0, y0 = t0, x1 = w * (1 - right), y1 = h * (1 - bottom), s = s }
end

-- What it draws in a window w by h (area: the video's place, given by the picture rig)
function M.draw(w, h, a)
    a = a or area(w, h)
    local s = a.s
    local u = ui.new(s)
    u.hover = hover
    local vw, vh = a.x1 - a.x0, a.y1 - a.y0
    local cw = math.min(560 * s, vw - 32 * s)
    local p = 24 * s
    local bw = 128 * s
    local tw = cw - 2 * p - 34 * s
    -- The steps laid out first, for the card's height
    local rows = {}
    local hh = 0
    for i, st in ipairs(M.steps()) do
        local width = tw - (st.button and (bw + 12 * s) or 0)
        local lines = ui.wrap(st.detail, width, 12, s)
        local rh = (24 + 16 * #lines + 14) * s
        if st.button then rh = math.max(rh, 46 * s) end
        rows[i] = { st = st, lines = lines, h = rh }
        hh = hh + rh
    end
    local ch = (84 + 40) * s + hh
    local x, y = a.x0 + (vw - cw) / 2, a.y0 + math.max(16 * s, (vh - ch) / 2)
    u:box(x + 2 * s, y + 4 * s, cw, ch, 16, "000000", { alpha = "80" })
    u:box(x, y, cw, ch, 16, C.panel, { border = 1 * s, border_color = C.line })
    u:hit(x, y, cw, ch, "card", function() end)
    u:text(x + p, y + 32 * s, "Welcome to Action Footage Player", { size = 18, bold = true })
    u:text(x + p, y + 56 * s, "Clips from your action camera, in color and steady, as they play.", { size = 12.5, color = C.dim })
    local yy = y + 84 * s
    u:box(x + p, yy - 6 * s, cw - 2 * p, math.max(1, s), 0, C.divider)
    for i, r in ipairs(rows) do
        local ry = yy + 8 * s
        local cx, cy = x + p + 11 * s, ry + 10 * s
        u:shape(ui.circle_path(cx, cy, 11 * s), C.accent)
        u:text(cx, cy + 0.5 * s, tostring(i), { align = 5, size = 12, bold = true, color = C.knob })
        local tx = x + p + 34 * s
        u:text(tx, cy, r.st.title, { size = 13.5, bold = true })
        local ly = cy + 20 * s
        for _, l in ipairs(r.lines) do
            u:text(tx, ly, l, { size = 12, color = C.dim })
            ly = ly + 16 * s
        end
        if r.st.button then
            local b = r.st.button
            u:button(x + cw - p - bw, bw, ry, b.id, b.label, b.cmd, { primary = b.id == "sample" })
        end
        yy = yy + r.h
    end
    u:text(x + p, y + ch - 22 * s, "Right-click for the menu  ·  Tab: the side panel", { size = 11, color = C.dim })
    return u
end

local function capture(on)
    if on == captured then return end
    captured = on
    if on then
        mp.add_forced_key_binding("MBTN_LEFT", "welcome-click", function(info)
            if info.event ~= "down" and info.event ~= "press" then return end
            local pos = mp.get_property_native("mouse-pos")
            if not pos then return end
            for i = #hits, 1, -1 do
                local t = hits[i]
                if pos.x >= t.x0 and pos.x < t.x1 and pos.y >= t.y0 and pos.y < t.y1 then
                    if type(t.cmd) == "function" then t.cmd() end
                    return
                end
            end
        end, { complex = true })
        mp.add_forced_key_binding("MBTN_LEFT_DBL", "welcome-click-dbl", function() end)
    else
        mp.remove_key_binding("welcome-click")
        mp.remove_key_binding("welcome-click-dbl")
    end
end

local function over(pos)
    if not pos or not pos.hover then return nil end
    for i = #hits, 1, -1 do
        local t = hits[i]
        if pos.x >= t.x0 and pos.x < t.x1 and pos.y >= t.y0 and pos.y < t.y1 then return t end
    end
end

function M.render()
    local d = mp.get_property_native("osd-dimensions")
    if not shown or not d or d.w <= 0 then
        overlay:remove()
        hits = {}
        capture(false)
        return
    end
    local u = M.draw(d.w, d.h)
    hits = u.hits
    overlay.res_x, overlay.res_y = d.w, d.h
    overlay.data = u:render()
    overlay:update()
    capture(over(mp.get_property_native("mouse-pos")) ~= nil)
end

function M.show()
    shown = true
    M.render()
end

function M.hide()
    if timer then timer:kill(); timer = nil end
    if not shown then return end
    shown = false
    hover = nil
    M.render()
end

function M.shown() return shown end

-- Idle with no file for SHOW_AFTER seconds: shown; a file starting: gone
mp.observe_property("idle-active", "bool", function(_, idle)
    if timer then timer:kill(); timer = nil end
    if idle then
        timer = mp.add_timeout(SHOW_AFTER, function()
            timer = nil
            if mp.get_property_native("idle-active") then M.show() end
        end)
    else
        M.hide()
    end
end)
mp.register_event("start-file", function() M.hide() end)

for _, p in ipairs({ "osd-dimensions", "video-margin-ratio-right", "video-margin-ratio-bottom", "fullscreen", "display-hidpi-scale" }) do
    mp.observe_property(p, "native", function() if shown then M.render() end end)
end
mp.observe_property("mouse-pos", "native", function(_, pos)
    if not shown then return end
    local t = over(pos)
    local h = t and t.id or nil
    if h ~= hover then hover = h; M.render() else capture(t ~= nil) end
end)

mp.register_script_message("welcome", function(what)
    if what == "hide" then M.hide() else M.show() end
end)

return M
