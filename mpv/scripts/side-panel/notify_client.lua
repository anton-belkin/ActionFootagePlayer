-- What a plugin loads to show what it does in the side panel's cards and to ask a name in its dialog (notify.lua,
-- docs/notifications.md). Required for its effect, from the plugin's main.lua (the plugins are side by side in
-- scripts/, so it's found beside them; without the side panel nothing changes):
--
--   pcall(function() package.path = package.path .. ";" .. mp.get_script_directory():gsub("[^/\\]*$", "") .. "side-panel/?.lua"
--                    require 'notify_client' end)
--
-- It adds to the plugin's `mp` (each plugin has its own):
--   mp.notify(t)          a card, as notify.lua's `notify` message takes it: { id, kind, title, detail, share, ... }
--   mp.ask(opts, done)    the name dialog: opts as notify.lua's `ask` takes it; done(text, cancelled). Returns false
--                         when there's no side panel to ask in (then the plugin asks in mpv's console, as before)
-- and routes the plugin's mp.osd_message there: a short note as a card (none while the panel is open, for what the
-- panel shows anyway), an error (Couldn't…, …failed) as an error card that stays until it's closed. Without the
-- side panel (an mpv the plugins were put into without it), mp.osd_message is mpv's own and mp.notify writes text.

local mp = require 'mp'
local utils = require 'mp.utils'

local M = {}
local plain = mp.osd_message
local asked = {}
local next_ask = 0

local function there()
    return mp.get_property_native("user-data/side_panel/loaded") == true
end
M.there = there

-- Errors, by how they're said
local ERROR_WORDS = { "^Couldn't", "^Can't", "[Ff]ailed", "isn't installed", "^Not saved", "couldn't be" }
function M.is_error(text)
    for _, p in ipairs(ERROR_WORDS) do if text:find(p) then return true end end
    return false
end

-- The first line a title, the rest its detail
function M.split(text)
    local title, rest = text:match("^([^\n]*)\n(.*)$")
    if not title then return text, nil end
    return title, rest
end

-- The responsiveness journal's action this card is for (docs/responsiveness-journal.md): the panel records it shown
local function journal_owner()
    local j = package.loaded.journal
    return j and j.owner and j.owner() or nil
end

function mp.notify(t)
    if type(t) ~= "table" then return end
    if t.journal == nil then t.journal = journal_owner() end
    if there() then
        mp.commandv("script-message-to", "side_panel", "notify", utils.format_json(t))
        return
    end
    -- No side panel: mpv's own text, as before
    if t.kind == "hide" then return plain("", 0) end
    local text = (t.title or "") .. (t.detail and ("\n" .. t.detail) or "")
    if t.kind == "working" and t.share then text = text .. string.format(" %d %%", math.floor(t.share * 100 + 0.5)) end
    plain(text, t.kind == "working" and 60 or t.kind == "error" and 8 or tonumber(t.seconds) or 3)
end

-- A plugin's own osd_message: a card. Each plugin's notes take one place (the next replaces it, as mpv's text did)
local NOTE_ID = "note:" .. mp.get_script_name()
local errors = 0
function M.route(text, seconds)
    text = tostring(text or "")
    if text == "" then return end
    local title, detail = M.split(text)
    seconds = tonumber(seconds)
    if M.is_error(title) then
        errors = errors + 1
        return mp.notify({ id = NOTE_ID .. ":error" .. errors, kind = "error", title = title, detail = detail })
    end
    -- A short one is a key's toggle or a confirmation the panel shows anyway: not while it's open
    mp.notify({ id = NOTE_ID, kind = "info", title = title, detail = detail, seconds = seconds,
                quiet = (seconds or 2) <= 2 })
end

mp.osd_message = function(text, seconds)
    if not there() then return plain(text, seconds) end
    M.route(text, seconds)
end

function mp.ask(opts, done)
    if not there() then return false end
    next_ask = next_ask + 1
    local id = mp.get_script_name() .. ":" .. next_ask
    asked[id] = done
    local req = {}
    for k, v in pairs(opts) do req[k] = v end
    req.id, req.reply = id, mp.get_script_name()
    req.journal = journal_owner()
    mp.commandv("script-message-to", "side_panel", "ask", utils.format_json(req))
    return true
end

mp.register_script_message("notify-answer", function(json)
    local r = utils.parse_json(json or "")
    if type(r) ~= "table" then return end
    local done = asked[r.id]
    asked[r.id] = nil
    if done then done(r.text or "", r.cancelled == true) end
end)

M.plain = plain
return M
