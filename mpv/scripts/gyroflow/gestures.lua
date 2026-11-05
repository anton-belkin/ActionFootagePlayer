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

function M.new(h)
    local g = { on = false, paused = false, drag = nil, messages = false, saved = nil }
    local function acting() return g.on and not g.paused end
    local prefix = h.prefix

    local function pan(dx, dy, x, y) h.pan(dx, dy, x, y) end

    -- The mouse: a drag pans, Alt+drag turns the horizon
    local function button(alt)
        return function(info)
            if not acting() then return end
            if info.event == "down" then
                local pos = mp.get_property_native("mouse-pos")
                if pos then g.drag = { x = pos.x, y = pos.y, alt = alt } end
            elseif info.event == "up" then
                g.drag = nil
                if h.released then h.released() end
            end
        end
    end

    mp.observe_property("mouse-pos", "native", function(_, pos)
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

    mp.register_script_message("gesture", function(kind, a, b, c)
        if not acting() then return end
        if kind == "scroll" then
            local phase = c
            if phase == "momentum" then return end
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
            h.zoom(1 + (tonumber(a) or 0))
        elseif kind == "rotate" then
            -- Counterclockwise as the fingers turn
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
            -- The app's mpv (tools/mpv-patches/0008); others have no such option
            self.messages = mp.set_property_native("macos-gesture-messages", true) == true
            for _, b in ipairs(self.bindings) do mp.add_forced_key_binding(b[1], b[2], b[3], b[4]) end
        else
            for _, b in ipairs(self.bindings) do mp.remove_key_binding(b[2]) end
            if self.saved ~= nil then mp.set_property_native("window-dragging", self.saved) end
            -- Paused, the other one still has them
            if self.messages and not self.paused then mp.set_property_native("macos-gesture-messages", false) end
            self.saved, self.messages, self.drag, self.paused = nil, false, nil, false
        end
    end

    function g:pause(paused)
        if paused == self.paused then return end
        self.paused, self.drag = paused, nil
        -- The other one gave the messages back as it left
        if not paused and self.on and self.messages then mp.set_property_native("macos-gesture-messages", true) end
    end

    return g
end

return M
