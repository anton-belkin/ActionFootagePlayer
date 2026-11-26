-- The side panel's part of the usage trace (docs/design/usage-trace.md), with the trace on (journal.conf trace=yes):
--   - the tab shown, written into the journal as it changes ("tab" records: a tab's name, "closed" or "hidden");
--   - "Something's wrong here": Alt+b, the flag on the panel's rail or the menu's item drop a marker in the journal,
--     save the window as it is beside it (mark-<when>-<pid>-<n>.png, screenshot-to-file … window) and then ask a line
--     about it in the name dialog (Enter with nothing, or Esc: the marker stays, without a note). tools/trace_report.py
--     lists the markers; the file-bug skill (.claude/skills/file-bug) starts from one.
-- With the trace off the key says so and the panel shows no flag.
--
--   local trace = require('trace').install(mp, J, notify)
--   trace.tab(name)        the tab shown now (each render: only a change is written)
--   trace.drop(via)        a marker, as Alt+b
--   trace.icon(u, cx, cy, color)   the flag, 20 points across
--   trace.on()             whether the trace is on

local M = {}

local KEY = "Alt+b"
M.KEY = KEY

function M.install(mp, J, notify)
    local utils = require 'mp.utils'
    local T = {}
    local last_tab = nil
    local asked = {}   -- the dialog's id -> the marker's

    function T.on() return J.trace == true end

    function T.tab(name)
        if not J.trace or name == last_tab then return end
        last_tab = name
        J.note("tab", { n = name })
    end

    local function ensure_dir(dir)
        if not dir or utils.file_info(dir) then return end
        local args = package.config:sub(1, 1) == "\\" and { "cmd", "/d", "/c", "if not exist \"" .. dir .. "\" mkdir \"" .. dir .. "\"" }
                     or { "mkdir", "-p", dir }
        mp.command_native({ name = "subprocess", args = args, playback_only = false, capture_stdout = true, capture_stderr = true })
    end

    local function ask_note(id)
        local key = "mark:" .. id
        asked[key] = id
        notify.ask({ id = key, reply = mp.get_script_name(), title = "Something's wrong here",
                     subtitle = "A line about it for the bug, if you like. Enter keeps it; Esc: no note",
                     placeholder = "What's wrong?", ok = "Keep", journal = J.owner() })
    end

    function T.drop(via)
        if not J.trace then
            return notify.notify({ id = "trace-off", kind = "info", seconds = 4, title = "The usage trace is off",
                                   detail = "journal.conf: trace=yes turns it on (docs/design/usage-trace.md)" })
        end
        local id, shot = J.mark({ via = via })
        if not id then return end
        if not shot then return ask_note(id) end
        ensure_dir(J.dir)
        -- The window as it is (the panel, the cards, what's over the video), before the dialog covers it
        mp.command_native_async({ "screenshot-to-file", shot, "window" }, function(ok, _, err)
            if not ok then J.mark_note(id, { shot = false, error = tostring(err or "screenshot failed") }) end
            ask_note(id)
        end)
    end

    mp.register_script_message("notify-answer", function(json)
        local r = utils.parse_json(json or "")
        if type(r) ~= "table" or not asked[r.id] then return end
        local id = asked[r.id]
        asked[r.id] = nil
        local text = type(r.text) == "string" and r.text or ""
        J.mark_note(id, { note = text ~= "" and text or nil, cancelled = r.cancelled == true or nil })
        notify.notify({ id = "mark-done", kind = "done", title = "Marked",
                        detail = text ~= "" and text or "Kept in the journal with a picture of the window" })
    end)

    mp.add_key_binding(KEY, "mark", function() T.drop("key") end)

    -- A flag on its pole
    function T.icon(u, cx, cy, color)
        local s = u.s
        local function at(x, y) return string.format("%.1f %.1f", cx + x * s, cy + y * s) end
        u:box(cx - 6.5 * s, cy - 8.5 * s, 1.6 * s, 17 * s, 0, color)
        u:shape("m " .. at(-4.9, -8.5) .. " l " .. at(7.5, -8.5) .. " " .. at(4.5, -4.5) .. " " .. at(7.5, -0.5) .. " " .. at(-4.9, -0.5),
                color)
    end

    return T
end

return M
