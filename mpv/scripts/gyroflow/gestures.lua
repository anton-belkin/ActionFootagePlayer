-- The mouse and the trackpad turning a view: what crop mode (crop.lua) and the 360 view (../view360/main.lua) share,
-- so the two turn the same way.
--
--   local gestures = require 'gestures'
--   local g = gestures.new({ prefix = "crop", pan = f, zoom = f, roll = f, reset = f, released = f, width = f,
--                            bindings = { { key, name, fn, flags }, ... } })
--   g:set(true)   the bindings and the trackpad's gestures taken; g:set(false) given back
--   g:pause(true) left alone while another set of gestures has them (the 360 view under crop mode); g:pause(false)
--                 taken again
--
-- A drag pans: pan(dx, dy, x, y), the picture moved by dx, dy screen pixels with the point at x, y (where the mouse is)
-- taken there from x - dx, y - dy. Alt+drag turns the horizon: roll(degrees), counterclockwise positive, 90° for a drag
-- across width() pixels (to the right: clockwise). The wheel, or a pinch, zooms: zoom(factor), above 1 closer. A double
-- click and Backspace: reset(). released() when the mouse button is let go after a drag.
--
-- On macOS the app's mpv sends the trackpad's gestures as script messages while a script asks for them
-- (--macos-gesture-messages, tools/mpv-patches/0008), so a two-finger scroll pans and a mouse wheel zooms. A trackpad's
-- scroll comes in phases (fingers on, moving, off); a mouse's wheel smoothed into fine steps (Logitech's Options+)
-- comes without, and zooms. The coasting after the fingers are off is left out: the picture stops where they did.
-- Elsewhere a wheel's whole clicks zoom and the fractions a touchpad sends pan, and Ctrl+wheel (a pinch, on Windows)
-- zooms.
--
-- The bindings are named <prefix>-key-<what>, apart from the script messages (mpv registers a binding's name as a
-- script message too). mpv sends the gestures' messages to every script: each set of gestures acts on them only while
-- it's on and not paused. The last one set on has the mouse (forced bindings: the newest are on top). One that's on
-- under another pauses, rather than giving back what the other has taken (the window's dragging, the gestures'
-- messages), and takes the messages again when the other is done.
--
-- Only over the video: with the mouse beside it (the side panel, the bar below: video-margin-ratio-*) the gestures
-- are left alone and the trackpad's messages turned off, so the panel scrolls by the wheel and the trackpad as usual
-- (and the app's VO, which turns the view by the messages, doesn't either). A drag that started over the video goes on
-- wherever the mouse goes.

local mp = require 'mp'

local M = {}

-- A wheel's click zooms by this much; a pinch's magnification as it comes
M.WHEEL_ZOOM = 1.1
-- A touchpad scroll's unit (mpv's wheel scale), in screen pixels, off macOS
M.SCROLL_PX = 40
-- A smooth wheel's points (macOS) for one click's zoom
M.SMOOTH_WHEEL_PT = 10

local function whole(scale)
    return scale and scale >= 1 and math.abs(scale - math.floor(scale + 0.5)) < 1e-6
end

-- Whether the mouse (mouse-pos) is over the video, not beside it in a margin (video-margin-ratio-*); d: osd-dimensions,
-- margins: { left, right, top, bottom } ratios. Outside the window (hover false), it's left as over it
function M.over_video(pos, d, margins)
    if not pos or pos.hover == false or not d or (d.w or 0) <= 0 or (d.h or 0) <= 0 then return true end
    local m = margins or {}
    local x0, x1 = d.w * (m.left or 0), d.w * (1 - (m.right or 0))
    local y0, y1 = d.h * (m.top or 0), d.h * (1 - (m.bottom or 0))
    return pos.x >= x0 and pos.x < x1 and pos.y >= y0 and pos.y < y1
end

local function margins()
    local function r(name) return mp.get_property_number("video-margin-ratio-" .. name) or 0 end
    return { left = r("left"), right = r("right"), top = r("top"), bottom = r("bottom") }
end

function M.new(h)
    local g = { on = false, paused = false, away = false, drag = nil, messages = false, saved = nil, input_time = nil, input_seq = 0 }
    local function acting() return g.on and not g.paused and (not g.away or g.drag ~= nil) end
    function g:acting() return acting() end
    local prefix = h.prefix

    local function pan(dx, dy, x, y) h.pan(dx, dy, x, y) end
    -- Whether the mouse is beside the video (set below, with the mouse's observer)
    local check_away

    -- The mouse: a drag pans, Alt+drag turns the horizon
    local function button(alt)
        return function(info)
            if not acting() then return end
            if info.event == "down" then
                local pos = mp.get_property_native("mouse-pos")
                if pos then g.drag = { x = pos.x, y = pos.y, alt = alt } end
            elseif info.event == "up" then
                local was = g.drag
                g.drag = nil
                if h.released then h.released() end
                -- Let go beside the video: it's the panel's again
                if was then check_away() end
            end
        end
    end

    -- The mouse beside the video or back over it: the trackpad's messages off there, on again here (only while these
    -- gestures have them: on and not paused)
    local function set_away(away)
        if away == g.away then return end
        g.away = away
        if g.on and not g.paused and g.messages then mp.set_property_native("macos-gesture-messages", not away) end
    end
    g.set_away = set_away
    check_away = function(pos)
        if g.drag then return end
        set_away(not M.over_video(pos or mp.get_property_native("mouse-pos"), mp.get_property_native("osd-dimensions"), margins()))
    end

    mp.observe_property("mouse-pos", "native", function(_, pos)
        check_away(pos)
        if not acting() or not g.drag or not pos then return end
        local dx, dy = pos.x - g.drag.x, pos.y - g.drag.y
        g.drag.x, g.drag.y = pos.x, pos.y
        if g.drag.alt then
            local w = h.width and h.width()
            -- To the right: clockwise
            if w and w > 0 then h.roll(-dx / w * 90) end
        else
            pan(dx, dy, pos.x, pos.y)
        end
    end)

    -- A wheel: up zooms in. A touchpad's fractions pan where the trackpad's gestures don't come as messages
    local function wheel(dx, dy, ctrl)
        return function(info)
            if not acting() or info.event == "up" then return end
            local scale = info.scale or 1
            if dx == 0 and (g.messages or ctrl or whole(scale)) then
                return h.zoom(M.WHEEL_ZOOM ^ (dy * scale))
            end
            pan(dx * scale * M.SCROLL_PX, dy * scale * M.SCROLL_PX)
        end
    end

    mp.register_script_message("gesture", function(kind, a, b, c, d, e)
        -- After the phase, the app's mpv sends the event's time and the message's number: the view shows the
        -- gestures up to it once it's applied (--macos-latency-log; the VO turns the view by the ones after it,
        -- tools/mpv-patches/0012). Every message counts, taken or not
        g.input_seq = tonumber(kind == "scroll" and e or d) or g.input_seq
        if not acting() then return end
        local t = tonumber(kind == "scroll" and d or c)
        if kind == "scroll" then
            local phase = c
            if phase == "momentum" then return end
            g.input_time = t
            local dx, dy = tonumber(a) or 0, tonumber(b) or 0
            -- A mouse's wheel, in fine steps: up zooms in
            if phase == "none" then
                if dy ~= 0 then h.zoom(M.WHEEL_ZOOM ^ (dy / M.SMOOTH_WHEEL_PT)) end
                return
            end
            local s = mp.get_property_number("display-hidpi-scale") or 1
            local pos = mp.get_property_native("mouse-pos") or {}
            pan(dx * s, dy * s, pos.x and pos.x + dx * s, pos.y and pos.y + dy * s)
        elseif kind == "magnify" then
            g.input_time = t
            h.zoom(1 + (tonumber(a) or 0))
        elseif kind == "rotate" then
            -- Counterclockwise as the fingers turn
            g.input_time = t
            h.roll(tonumber(a) or 0)
        end
    end)

    local function name(what) return prefix .. "-key-" .. what end
    g.bindings = {
        { "MBTN_LEFT", name("drag"), button(false), { complex = true } },
        { "Alt+MBTN_LEFT", name("roll"), button(true), { complex = true } },
        { "MBTN_LEFT_DBL", name("reset-dbl"), function() if acting() then h.reset() end end },
        { "BS", name("reset"), function() if acting() then h.reset() end end },
        { "WHEEL_UP", name("wheel-up"), wheel(0, 1), { complex = true } },
        { "WHEEL_DOWN", name("wheel-down"), wheel(0, -1), { complex = true } },
        { "WHEEL_LEFT", name("wheel-left"), wheel(1, 0), { complex = true } },
        { "WHEEL_RIGHT", name("wheel-right"), wheel(-1, 0), { complex = true } },
        { "Ctrl+WHEEL_UP", name("pinch-in"), wheel(0, 1, true), { complex = true } },
        { "Ctrl+WHEEL_DOWN", name("pinch-out"), wheel(0, -1, true), { complex = true } },
    }
    for _, b in ipairs(h.bindings or {}) do table.insert(g.bindings, b) end

    -- The bindings and the gestures taken (on) or given back
    function g:set(on)
        if on == self.on then return end
        self.on = on
        if on then
            self.saved = mp.get_property_native("window-dragging")
            mp.set_property_native("window-dragging", false)
            -- The app's mpv (tools/mpv-patches/0008); others have no such option. Off while the mouse is beside the
            -- video (turned on from the panel, it is)
            self.away = not M.over_video(mp.get_property_native("mouse-pos"), mp.get_property_native("osd-dimensions"), margins())
            self.messages = mp.set_property_native("macos-gesture-messages", not self.away) == true
            for _, b in ipairs(self.bindings) do mp.add_forced_key_binding(b[1], b[2], b[3], b[4]) end
        else
            for _, b in ipairs(self.bindings) do mp.remove_key_binding(b[2]) end
            if self.saved ~= nil then mp.set_property_native("window-dragging", self.saved) end
            -- Paused, the other one still has them
            if self.messages and not self.paused then mp.set_property_native("macos-gesture-messages", false) end
            self.saved, self.messages, self.drag, self.paused, self.away = nil, false, nil, false, false
        end
    end

    function g:pause(paused)
        if paused == self.paused then return end
        self.paused, self.drag = paused, nil
        -- The other one gave the messages back as it left
        if not paused and self.on and self.messages then mp.set_property_native("macos-gesture-messages", not self.away) end
    end

    return g
end

return M
