-- tagging: tags and culling on what plays (tags.lua, docs/design/tags-and-culling.md), synced with the library's
-- stores. main.lua wires its keys and messages
local mp = require 'mp'
local utils = require 'mp.utils'
local link = require 'link'

local M = {}
-- Set by install (main.lua's W)
local W, state, fs, ws, sidecar, catalog, input, windows, macos
local function publish(...) return W.publish(...) end
local function mkdir(...) return W.mkdir(...) end
local function stamp(...) return W.stamp(...) end
local function plugin_tool(...) return W.plugin_tool(...) end
local function rewrite_link(...) return W.rewrite_link(...) end

-- ---------- Tags and culling (tags.lua, docs/design/tags-and-culling.md) ----------
-- What plays is tagged: the original (also while its clip preview plays), or the moment playing. An export has
-- nothing to tag. Published as user-data/workspace/tags: { kind (original, moment), pick, rating, list (custom tags),
-- recent (the tags used last on this computer, the Library's too), stores (the library's enabled stores this computer
-- syncs: "finder") }.
--   P / X / U             accept / reject / neither (P or X again: neither)
--   1–5, 0                that many stars (the same again: none), none
--   K                     a custom tag typed: on, or off when on (tags.lua's parse: +a, -a, *3 stars, commas)
-- Script messages: tag <text> (as typed at K), tag-toggle <name>, tag-rate <0-5>, tag-pick <accept|reject|none>,
-- tag-ask (K). Each enabled store is synced both ways as the item loads and after each change (three-way, tags.lua's
-- merge). The Library's changes come through heard (inbox.lua): read again.
local tags_lib = require 'tags'
local function support_dir()
    local base = windows and os.getenv("APPDATA") or macos and (os.getenv("HOME") or "") .. "/Library/Application Support"
                 or os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") or "") .. "/.config"
    return ws.join(base, "ActionFootage")
end
-- The tags used last, this computer's (both apps write it): { "recent": [...] }
local function recent_file() return ws.join(support_dir(), "tags-recent.json") end
local function recent_tags() local d = fs.read_json(recent_file()); return tags_lib.list(d and d.recent) end
local function remember_tags(used)
    if #used == 0 then return end
    mkdir(support_dir())
    local f = io.open(recent_file() .. ".tmp", "w")
    if not f then return end
    f:write(utils.format_json({ recent = tags_lib.recent(recent_tags(), used) }), "\n")
    f:close()
    os.remove(recent_file())
    os.rename(recent_file() .. ".tmp", recent_file())
end

-- What plays, to tag: { kind, file (the original, the moment's file or the photo) }; nil for an export, a stream. A
-- photo (its link, or one the player made, with its link) is tagged itself, as a moment is: its pick and rating in its
-- link, its tags in its XMP (afp-meta, as the Library writes them); a photo without a link isn't tagged here
local function tag_target()
    if not state.clip or state.clip:find("://", 1, true) or state.export then return nil end
    if state.moment and state.moment.file then return { kind = "moment", file = state.moment.file } end
    if state.photo and state.photo.file then return { kind = "photo", file = state.photo.file } end
    if type(mp.get_property_native("user-data/afp/photo")) == "table" then return nil end
    return { kind = "original", file = state.clip }
end

-- The target's record: a moment's link, or the original's own file (and the file that holds it, for its time)
local function tags_read(t)
    if t.kind == "original" then
        local doc = sidecar.read(t.file)
        return type(doc) == "table" and doc or {}, sidecar.path(t.file)
    end
    if t.kind == "photo" then
        local l = utils.parse_json(fs.read_photo_link(t.file) or "")
        l = type(l) == "table" and l or {}
        l.tags = link.photo_tags(t.file)
        return l, t.file
    end
    local l = utils.parse_json(fs.read_link(t.file) or "")
    return type(l) == "table" and l or {}, t.file
end

-- The record changed by fn(record) (true: changed): the original's own file, or the moment's link
local function tags_write(t, fn)
    if t.kind == "original" then
        return sidecar.update(t.file, function(d) local c = fn(d); if c then stamp(d) end; return c end)
    end
    local doc = tags_read(t)
    if not fn(doc) then return true end
    if t.kind == "photo" then
        -- Its pick and rating in its link, its tags in its XMP: afp-meta writes both (a HEIC's through afp-photo)
        local meta = plugin_tool("afp-meta")
        if not meta then return false, "afp-meta isn't installed" end
        local tags = doc.tags or {}
        doc.tags = nil
        local w = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true,
                                      args = { meta, t.file, "--link", utils.format_json(doc), "--tags", utils.format_json(tags) } })
        if not (w and w.status == 0) then return false, "couldn't write its link and tags (afp-meta: " .. tostring(w and w.stderr) .. ")" end
        catalog.note(state.project, { t.file })
        return true
    end
    local ok = rewrite_link(t.file, function(l) fn(l) end)
    return ok, not ok and "couldn't write its link (afp-cut)" or nil
end

-- The stores this computer syncs for the project (its own file's tag_stores: fs.tag_stores): Finder's on a Mac
local function enabled_stores()
    if not macos or not state.project then return {} end
    for _, s in ipairs(fs.tag_stores(state.project)) do
        if s == "finder" then return { "finder" } end
    end
    return {}
end

-- A file's Finder tags (strings) and its change time
local function finder_read(file)
    local r = mp.command_native({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true,
                                  args = { "/usr/bin/xattr", "-px", tags_lib.FINDER_XATTR, file } })
    local info = utils.file_info(file)
    local list = (r and r.status == 0) and tags_lib.finder_parse(tags_lib.unhex(r.stdout) or "") or {}
    return list, info and info.ctime or 0
end
local function finder_write(file, list)
    local args = #list == 0 and { "/usr/bin/xattr", "-d", tags_lib.FINDER_XATTR, file }
        or { "/usr/bin/xattr", "-wx", tags_lib.FINDER_XATTR, tags_lib.hex(tags_lib.finder_plist(list)), file }
    local w = mp.command_native({ name = "subprocess", playback_only = false, capture_stderr = true, args = args })
    if not (w and w.status == 0) and #list > 0 then mp.msg.warn("couldn't write Finder tags on " .. file .. ": " .. tostring(w and w.stderr)) end
end

-- The original's file whose Finder tags are its: the copy beside its own file (the archive's, a working copy playing)
local function store_file(t)
    if t.kind ~= "original" then return t.file end
    local own = sidecar.path(t.file)
    local ext = t.file:match("(%.[^./\\]+)$") or ""
    local main = own and own:gsub("%.afp%.json$", "") .. ext
    return (main and fs.is_file(main)) and main or t.file
end

-- The item synced with each enabled store, three-way: what changed outside comes in, what changed here goes out.
-- What came in, for the OSD
local function tags_sync(t)
    local came = nil
    for _, store in ipairs(enabled_stores()) do
        if store == "finder" then
            local file = store_file(t)
            if fs.is_file(file) then
                local now, ctime = finder_read(file)
                local rec, rec_file = tags_read(t)
                local info = rec_file and utils.file_info(rec_file)
                local base = type(rec.synced) == "table" and type(rec.synced.finder) == "table" and tags_lib.from_finder(rec.synced.finder) or nil
                local lib = tags_lib.model(rec)
                local merged = tags_lib.merge(base, lib, tags_lib.from_finder(now), ctime > (info and info.mtime or 0) + 1)
                local strings = tags_lib.to_finder(merged, now)
                local agreed = #strings > 0 and strings or nil
                local had = type(rec.synced) == "table" and rec.synced.finder or nil
                -- The record first (a link written again may be a new file), then Finder's
                if not tags_lib.same(merged, lib) or table.concat(had or {}, "\0") ~= table.concat(agreed or {}, "\0") then
                    tags_write(t, function(d)
                        tags_lib.put(d, merged)
                        d.synced = type(d.synced) == "table" and d.synced or {}
                        d.synced.finder = agreed
                        if next(d.synced) == nil then d.synced = nil end
                        return true
                    end)
                    if not tags_lib.same(merged, lib) then came = "Finder" end
                end
                if table.concat(strings, "\0") ~= table.concat(now, "\0") then finder_write(file, strings) end
            end
        end
    end
    return came
end

local function tags_reload(sync)
    local t = tag_target()
    if not t then state.tags = nil; return publish() end
    if sync then
        local came = tags_sync(t)
        -- (taken in quietly: the panel's chips show them)
        if came then mp.msg.info("tags changed in " .. came .. ": taken into the library") end
    end
    local m = tags_lib.model(tags_read(t))
    state.tags = { kind = t.kind, pick = m.pick, rating = m.rating, list = m.tags, recent = recent_tags(), stores = enabled_stores() }
    publish()
end

-- What was asked (tags.lua's parse, { pick }, { rating }) done to what plays, then synced to the stores
local function tags_change(op)
    local t = tag_target()
    if not t then return mp.osd_message(state.export and "An export isn't tagged: its moment or original is" or "Nothing to tag here", 3) end
    local before = tags_lib.model(tags_read(t))
    local ok, err = tags_write(t, function(d) return tags_lib.apply(d, op) end)
    if not ok then
        mp.osd_message("Tags not saved: " .. tostring(err), 5)
        return tags_reload(false)
    end
    tags_sync(t)
    local after = tags_lib.model(tags_read(t))
    local parts = {}
    if after.pick ~= before.pick then parts[#parts + 1] = after.pick == "accept" and "✓ Accepted" or after.pick == "reject" and "✕ Rejected" or "Not picked" end
    if after.rating ~= before.rating then parts[#parts + 1] = after.rating and string.rep("★", after.rating) or "No stars" end
    local added = {}
    for _, a in ipairs(after.tags) do if not tags_lib.find(before.tags, a) then added[#added + 1] = a; parts[#parts + 1] = "+" .. a end end
    for _, b in ipairs(before.tags) do if not tags_lib.find(after.tags, b) then parts[#parts + 1] = "−" .. b end end
    remember_tags(added)
    if #parts > 0 then mp.osd_message((t.kind == "moment" and "Moment: " or t.kind == "photo" and "Photo: " or "Original: ") .. table.concat(parts, "  "), 2) end
    tags_reload(false)
end

-- K: a tag typed, the recent ones offered (Tab completes the one being typed)
local function tags_ask()
    local t = tag_target()
    if not t then return mp.osd_message("Nothing to tag here", 2) end
    local now = state.tags and state.tags.list or {}
    local offer = {}
    for _, r in ipairs(recent_tags()) do offer[#offer + 1] = r end
    for _, n in ipairs(now) do if not tags_lib.find(offer, n) then offer[#offer + 1] = n end end
    -- The side panel's dialog: the recent tags as its earlier names (one clicked puts it in: on, or off if it's on)
    if mp.ask({ title = "Tag the " .. t.kind, subtitle = #now > 0 and ("Now: " .. table.concat(now, ", ")) or "No tags yet",
                placeholder = "A tag (one it has: off); -tag takes it off; commas for more", names = offer, ok = "Tag" },
              function(text, cancelled)
                  if not cancelled and text ~= "" then tags_change(tags_lib.parse(text)) end
              end) then
        return
    end
    if not input then return mp.osd_message("Typing a tag needs mpv 0.38 or later", 3) end
    input.get({
        prompt = "Tag the " .. t.kind .. (#now > 0 and (" [" .. table.concat(now, ", ") .. "]") or "") .. " (a tag on/off, -off):",
        id = "workspace-tags",
        complete = function(before)
            local head, word = before:match("^(.*,%s*[%+%-]?)(.*)$")
            if not head then head, word = before:match("^([%+%-]?)(.*)$") end
            local list = {}
            for _, o in ipairs(offer) do if o:lower():sub(1, #word) == word:lower() then list[#list + 1] = o end end
            return list, #head + 1
        end,
        submit = function(text)
            input.terminate()
            text = (text or ""):gsub("^%s+", ""):gsub("%s+$", "")
            if text ~= "" then tags_change(tags_lib.parse(text)) end
        end,
    })
end

local function pick(which)
    -- The same again: neither
    if which ~= "none" and state.tags and state.tags.pick == which then which = "none" end
    tags_change({ pick = which })
end
local function rate(n)
    n = tonumber(n) or 0
    if n > 0 and state.tags and state.tags.rating == n then n = 0 end
    tags_change({ rating = n })
end


function M.tag(text) if text and text ~= "" then tags_change(tags_lib.parse(text)) end end
function M.toggle(name) if tags_lib.clean(name) then tags_change({ toggle = { name } }) end end
function M.pick_message(which) if which == "accept" or which == "reject" or which == "none" then pick(which) end end
M.ask, M.pick, M.rate, M.reload = tags_ask, pick, rate, tags_reload

function M.install(w)
    W = w
    state, fs, ws, sidecar, catalog, input = W.state, W.fs, W.ws, W.sidecar, W.catalog, W.input
    windows, macos = W.windows, W.macos
    fs.tags_reload = function() tags_reload(true) end
    -- A moment written again (k on one playing: a new file renamed over the old, its attributes gone): synced again, so
    -- its Finder tags come back from its link, which kept them
    fs.tags_refollow = function(file)
        if state.moment and state.moment.file == file then tags_reload(true) end
    end
end

return M
