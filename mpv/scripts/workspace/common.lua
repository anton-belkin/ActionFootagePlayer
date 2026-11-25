-- common: what the workspace plugin's parts all use (main.lua wires them): small helpers, the name dialog, a folder
-- made, the tools beside the gyroflow plugin (gyroflow-project, afp-cut, a file's fingerprint), a link rewritten,
-- the transform as it plays, the project a command needs
local mp = require 'mp'
local utils = require 'mp.utils'
local longpath = require 'longpath'
local link = require 'link'
local transform = require 'transform'

local M = {}
-- Set by install (main.lua's W)
local state, fs, ws, windows, macos, input, catalog, moments, pf
local plugin_tool, fingerprint, rewrite_link

local function copy(t)
    local out = {}
    for k, v in pairs(type(t) == "table" and t or {}) do out[k] = v end
    return out
end

local function clock(s)
    s = math.floor(s + 0.5)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

local function run(args)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end)
end

-- Typed text: the console's (mp.input, mpv 0.38 and later), else none (the suggestion is taken). opts: prompt,
-- suggestion (Tab puts it in), default (the field filled), history (a file of earlier names, ↑)
-- In the side panel's dialog (opts.title, subtitle, suggestion, names, ok, ok_typed, placeholder: side-panel/notify.lua),
-- else in mpv's console (opts.prompt); Esc: done isn't called
local function ask(opts, done)
    -- What follows the name never fails in silence: an error is said on screen (and is in the log)
    local function after(text)
        local ok, err = xpcall(function() done(((text or ""):gsub("^%s+", ""):gsub("%s+$", ""))) end, debug.traceback)
        if not ok then
            mp.msg.error("after the name typed: " .. tostring(err))
            mp.notify({ id = "workspace:ask", kind = "error", title = "That didn't work", detail = tostring(err):match("^[^\n]*") })
        end
    end
    if opts.title and mp.ask({ title = opts.title, subtitle = opts.subtitle, value = opts.default, suggestion = opts.suggestion,
                               names = opts.names, ok = opts.ok, ok_typed = opts.ok_typed, placeholder = opts.placeholder },
                             function(text, cancelled) if not cancelled then after(text) end end) then
        return
    end
    if not input then return done("") end
    local id = opts.id or "workspace"
    input.get({
        prompt = opts.prompt,
        id = id,
        default_text = opts.default,
        cursor_position = opts.default and #opts.default + 1 or nil,
        history_path = opts.history,
        complete = opts.suggestion and function() return { opts.suggestion }, 1 end or nil,
        submit = function(text)
            input.terminate()
            after(text)
        end,
    })
end

-- The project's own file read (projectfile.lua), and a change to it; nil and false without a project
local function project_doc() return state.project and pf.read(state.project) or nil end
local function project_update(fn) return state.project ~= nil and pf.update(state.project, fn) end

local function mkdir(dir)
    if windows then
        mp.command_native({ name = "subprocess", args = { "cmd", "/c", "mkdir", longpath.extended(dir) }, playback_only = false })
    else
        mp.command_native({ name = "subprocess", args = { "mkdir", "-p", dir }, playback_only = false })
    end
    return fs.is_dir(dir)
end

-- gyroflow-project, beside the gyroflow plugin's gyroflow-export
local function project_tool()
    local exporter = mp.get_property_native("user-data/gyroflow/exporter")
    if type(exporter) ~= "string" then return nil end
    return (exporter:gsub("gyroflow%-export(%.?%a*)$", "gyroflow-project%1"))
end

local function run_tool(args)
    local r = mp.command_native({ name = "subprocess", args = args, playback_only = false, capture_stderr = true })
    if not r or r.status ~= 0 then
        mp.msg.warn(table.concat(args, " ") .. ": " .. tostring(r and r.stderr))
        return false
    end
    return true
end

-- The transform bundle as it plays (the gyroflow plugin's), whole: what a moment, a clip preview or the source keeps.
-- What a Gyroflow project owns (and a wider view or none over it, for viewing only) stays as it was saved there, else
-- the session's: the project decides it, and it's kept only to come back if the project goes
local function current_view()
    local g = mp.get_property_native("user-data/gyroflow")
    if type(g) ~= "table" then return copy(state.saved) end
    local v = {}
    for _, k in ipairs(transform.KEYS) do v[k] = g[k] end
    -- The gyroflow plugin's `lens` is the lens profile; the bundle's lens (corrected, as recorded) is its lens_mode
    v.lens = g.lens_mode
    if g.kind == "project" then
        local keep = state.saved or g.session or {}
        for k in pairs(transform.PROJECT_KEYS) do v[k] = keep[k] end
    end
    return (transform.complete(v, transform.session(mp.get_property_native)))
end

-- The clip's project, for what needs one (moments, exports, clip previews). None is made by itself: the side panel's
-- Project tab opens, where one is initialized in a folder of the user's choice (init-project). Marks need none
local function need_project(what)
    if state.project then return state.project end
    mp.commandv("script-message-to", "side_panel", "tab", "project")
    mp.osd_message(what .. " needs the project: initialize it first, in the folder of your choice\n"
        .. "(the side panel's Project tab, or right click: Project)", 5)
    return nil
end

local function reveal(file, folder)
    if macos then run(folder and { "open", file } or { "open", "-R", file })
    elseif windows then run(folder and { "explorer", file } or { "explorer", "/select," .. file })
    else run({ "xdg-open", folder and file or (ws.parent(file) or ".") }) end
end

-- A tool of the gyroflow plugin beside this one (the app has both)
plugin_tool = function(name)
    local dir = mp.get_script_directory()
    local plugins = dir and dir:match("^(.*)[/\\][^/\\]+[/\\]?$")
    if not plugins then return nil end
    local exe = utils.join_path(utils.join_path(plugins, "gyroflow"), name .. (windows and ".exe" or ""))
    return fs.is_file(exe) and exe or nil
end

-- A file's fingerprint (gyroflow-export --fingerprint: SHA-256 of its first and last MiB), as links keep their
-- original's (gyroflow-export adds it) and marks files their clip's, to find it again once it's moved or renamed
local function fingerprint_args(file)
    local exe = plugin_tool("gyroflow-export")
    return exe and { name = "subprocess", playback_only = false, capture_stdout = true, args = { exe, "--fingerprint", file } }
end
local function fingerprint_of(r)
    local h = r and r.status == 0 and type(r.stdout) == "string" and r.stdout:match("^(%x+)")
    return h and #h == 64 and h or nil
end
fingerprint = function(file)
    local cmd = fingerprint_args(file)
    return cmd and fingerprint_of(mp.command_native(cmd)) or nil
end

-- A link's JSON rewritten in place (afp-cut --set-key), fn changing it; true when written
rewrite_link = function(file, fn)
    local exe = plugin_tool("afp-cut")
    local raw = exe and fs.read_link(file)
    local l = raw and utils.parse_json(raw)
    if type(l) ~= "table" then return false end
    fn(l)
    local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                                  args = { exe, "--set-key", file, link.KEY, utils.format_json(l) } })
    if not (r and r.status == 0) then mp.msg.warn("couldn't write the link of " .. file .. ": " .. tostring(r and r.stderr)) end
    -- Written in place: noted (its folder touched, so a look lists it again whatever happens)
    if r and r.status == 0 then catalog.note(moments.project_of(file), { file }) end
    return r and r.status == 0
end

function M.install(W)
    state, fs, ws, windows, macos, input = W.state, W.fs, W.ws, W.windows, W.macos, W.input
    catalog, moments, pf = W.catalog, W.moments, W.pf
    fs.fingerprint = function(file) return fingerprint and fingerprint(file) or nil end
    W.copy, W.clock, W.run, W.ask, W.mkdir, W.project_tool, W.run_tool = copy, clock, run, ask, mkdir, project_tool, run_tool
    W.current_view, W.need_project, W.reveal, W.plugin_tool = current_view, need_project, reveal, plugin_tool
    W.fingerprint_args, W.fingerprint_of, W.fingerprint, W.rewrite_link = fingerprint_args, fingerprint_of, fingerprint, rewrite_link
end

return M
