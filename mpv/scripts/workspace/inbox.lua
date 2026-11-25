-- inbox: what Action Footage Library changes, heard at once (afp-catalog's live.rs), for the workspace plugin.
-- main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, sidecar, windows, macos, shown_time
local function refresh(...) return W.refresh(...) end
local function recount(...) return W.recount(...) end
local function mkdir(...) return W.mkdir(...) end

-- What Action Footage Library changes, heard at once when it runs on this computer (afp-catalog's live.rs): a window
-- has an inbox, <cache>/live/player-<pid>.in, the Library appends a line { project, files } to for each change of its
-- (a moment renamed or removed, a tag, a lens type, a Take in...), and this looks at its size four times a second.
-- Then the clip's moments are asked again, the project counted again, and a lens type changed for the clip playing
-- plays at once (its own file says how it's stabilized). A batch (no window) has none
local function live_dir()
    local home = os.getenv("AFP_HOME")
    if home and home ~= "" then return utils.join_path(utils.join_path(home, "cache"), "live") end
    local base
    if windows then
        base = os.getenv("LOCALAPPDATA") or os.getenv("APPDATA")
    elseif macos then
        base = os.getenv("HOME") and utils.join_path(os.getenv("HOME"), "Library/Caches")
    else
        base = os.getenv("XDG_CACHE_HOME") or (os.getenv("HOME") and utils.join_path(os.getenv("HOME"), ".cache"))
    end
    return base and utils.join_path(utils.join_path(base, "ActionFootage"), "live")
end
local function same_path(a, b)
    local function norm(p) p = tostring(p or ""):gsub("\\", "/"):gsub("/+$", ""); return windows and p:lower() or p end
    return norm(a) == norm(b)
end
local function lens_type_of(clip)
    local doc = clip and sidecar.read(clip)
    return type(doc) == "table" and type(doc.source) == "table" and doc.source.lens_type or nil
end

local inbox, inbox_at, lens_loaded = nil, 0, nil
function M.loaded() lens_loaded = lens_type_of(state.clip) end
local function heard(msg)
    if type(msg) ~= "table" or not state.project then return end
    local files = type(msg.files) == "table" and msg.files or {}
    if not same_path(msg.project, state.project) then
        local ours = false
        for _, f in ipairs(files) do if ws.relative(f, state.project) then ours = true end end
        if not ours then return end
    end
    mp.msg.info("the Library changed " .. #files .. " file(s) of the project: read again")
    refresh()
    if recount then recount() end
    -- Tags set in the Library on what plays: shown
    fs.tags_reload()
    -- The clip's own file: a lens type chosen in the Library is how it plays, from now (the clip loaded again where it
    -- is; a moment playing shows it the next time it's opened)
    local own = state.sidecar_file
    for _, f in ipairs(files) do
        if own and same_path(f, own) then
            local now = lens_type_of(state.clip)
            if now ~= lens_loaded then
                lens_loaded = now
                if not state.moment and not state.offset and state.clip then
                    mp.msg.info("lens type " .. tostring(now) .. " from the Library: the clip loaded again")
                    W.wanted = { file = state.clip, ["in"] = shown_time() }
                    mp.commandv("loadfile", state.clip, "replace")
                else
                    mp.msg.info("lens type changed in the Library: the clip plays with it when opened again")
                end
            end
        end
    end
end
-- The inbox made and looked at (not in a batch); gone when the player quits
function M.start()
    if not os.getenv("AFP_BATCH") or os.getenv("AFP_BATCH") == "" then
        local dir = live_dir()
        if dir and (fs.is_dir(dir) or mkdir(dir)) then
            inbox = utils.join_path(dir, "player-" .. tostring(utils.getpid()) .. ".in")
            local f = io.open(inbox, "ab")
            if f then f:close() else inbox = nil end
        end
        if inbox then
            mp.add_periodic_timer(0.25, function()
                local info = utils.file_info(inbox)
                if not info or info.size <= inbox_at then return end
                local f = io.open(inbox, "rb")
                if not f then return end
                f:seek("set", inbox_at)
                local text = f:read("*a") or ""
                f:close()
                -- Whole lines only: one being written is read next time
                local upto = text:match("^.*()\n")
                if not upto then return end
                inbox_at = inbox_at + upto
                for line in text:sub(1, upto):gmatch("[^\r\n]+") do heard(utils.parse_json(line)) end
            end)
            mp.register_event("shutdown", function() os.remove(inbox) end)
        end
    end
end

function M.install(w)
    W = w
    state, fs, ws, sidecar, windows, macos, shown_time = W.state, W.fs, W.ws, W.sidecar, W.windows, W.macos, W.shown_time
end

return M
