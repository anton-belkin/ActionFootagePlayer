-- A list opened from Action Footage Library: what the Library showed when an item was double-clicked (its search,
-- filters, sort), as the playlist, the item opened playing. Previous and Next then walk that list, in the Library's
-- order. It's a snapshot: the Library doesn't follow it afterwards. Another opened from the Library replaces it, as
-- a video opened from Finder does.
--
-- The Library writes it as an M3U file, <its cache>/lists/<time>.afplist.m3u8 (library/src/main.rs, open_list):
--
--   #EXTM3U
--   #PLAYLIST:Originals › Egypt                 what the Library showed (the side panel's playlist says it)
--   #AFP-START:17                               the item opened, counted from 0
--   #EXTINF:-1,Turtle at the wreck              each item's name (a moment's own, not its file's)
--   /Volumes/ssd/…/_Moments/Turtle at the wreck.afplink.mp4
--
-- and opens it with the player as a file (macOS: as Finder would, an Apple Event; Windows: the launcher, which
-- sends it to the running player). mpv reads the M3U into the playlist itself; this file only:
--   - starts it at the item opened: playlist-start, for that load only, which mpv applies as it puts the list in
--     (nothing before the item is loaded first);
--   - keeps mpv from making the playlist of the lists' folder when the list is opened by itself (the older lists
--     came back into the playlist before it);
--   - keeps mpv from making the playlist of the item's folder instead (autocreate-playlist, the app's setting), which
--     it would for a list of one;
--   - says what the list is (user-data/library-list: its file, label and length) for the side panel.
-- A moment in it plays as a moment, a clip preview as its original: the workspace plugin's own on_load (later than
-- this one) does that as for any playlist.
--
-- The parsing is plain Lua (tests/librarylist_test.lua); install() is mpv's part.

local M = {}

M.EXT = ".afplist.m3u8"

-- Whether a path is such a list (the file's name says it)
function M.is_list(path)
    path = type(path) == "string" and path:lower() or ""
    return path:sub(-#M.EXT) == M.EXT
end

-- The list's head: its start (0-based) and label, and how many items it holds. Nil when it isn't one of the Library's
function M.parse(text)
    if type(text) ~= "string" or text:sub(1, 7) ~= "#EXTM3U" then return nil end
    local start, label, count = 0, nil, 0
    for line in text:gmatch("[^\r\n]+") do
        local n = line:match("^#AFP%-START:(%d+)%s*$")
        if n then start = tonumber(n) end
        local l = line:match("^#PLAYLIST:(.*)$")
        if l then label = l end
        if line ~= "" and not line:match("^#") then count = count + 1 end
    end
    if count == 0 then return nil end
    if start >= count then start = count - 1 end
    return { start = start, label = label, count = count }
end

local function read(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local text = f:read("*a")
    f:close()
    return text
end

function M.install(mp)
    -- Before the workspace plugin's own on_load (10): what's set here is there when it decides
    mp.add_hook("on_load", 5, function()
        local path = mp.get_property("path", "")
        if M.is_list(path) then
            -- The list itself, about to be read into the playlist: from the item opened
            local list = M.parse(read(path))
            if not list then return end
            -- Opened by itself, mpv would first make the playlist of the lists' folder (the Library's older lists)
            mp.set_property("file-local-options/autocreate-playlist", "no")
            mp.set_property("file-local-options/playlist-start", tostring(list.start))
            mp.set_property_native("user-data/library-list", { file = path, label = list.label or "", count = list.count })
            return
        end
        -- An item of it: played as itself, its folder not made the playlist
        if M.is_list(mp.get_property("playlist-path", "")) then
            mp.set_property("file-local-options/autocreate-playlist", "no")
        end
    end)
end

return M
