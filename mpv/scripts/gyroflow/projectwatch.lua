-- Ctrl+e's watch on the Gyroflow project (main.lua's check_project): whether Gyroflow saved it since the version the
-- player plays, and when to take a save up. Every WATCH_INTERVAL main.lua asks step() what to do.
--
-- A version is the file's "mtime/size", and what's in it while it's fresh (written within FRESH seconds of now; nil
-- after, so a project left alone isn't read again and again from a NAS). mpv's mtimes are whole seconds: a second
-- save in the same second with the same size (a value changed in place, 0.75 -> 0.55) is told apart by what's in it.
-- Gyroflow writes the file in place: a version is taken once it stayed the same for an interval

local M = {}

M.FRESH = 2

-- The file at `path` as it is now, nil when there's none. `utils` is mp.utils; `now` os.time() (for the tests)
function M.version(utils, path, now)
    local info = utils.file_info(path)
    if not (info and info.is_file) then return nil end
    local v = { stamp = string.format("%s/%d", tostring(info.mtime), info.size) }
    if math.abs((now or os.time()) - (tonumber(info.mtime) or 0)) <= M.FRESH then
        local f = io.open(path, "rb")
        if f then
            v.content = f:read("*a")
            f:close()
        end
    end
    return v
end

-- The same save: the same stamp, and what's in them the same where both were read
function M.same(a, b)
    if a == nil or b == nil then return a == b end
    return a.stamp == b.stamp and (a.content == nil or b.content == nil or a.content == b.content)
end

-- What to do with the project's version `v` now (nil: there's none). `watch` = { project (nil while waiting for one
-- beside a clip that had none), version (the one playing), pending, saves }. Returns nil (nothing yet), "found" (a
-- project appeared and stayed the same: load the video again with it), or "reload", version (saved again and stayed
-- the same: reload with it; the version is a new string for each save taken, "mtime/size#n", so the filter, the
-- shader's reuse key and the table server all see a project they haven't loaded)
function M.step(watch, v)
    if not v or M.same(v, watch.version) then
        watch.pending = nil
        return nil
    end
    if not M.same(v, watch.pending) then
        watch.pending = v
        return nil
    end
    watch.version, watch.pending = v, nil
    if not watch.project then return "found" end
    watch.saves = (watch.saves or 0) + 1
    return "reload", v.stamp .. "#" .. watch.saves
end

return M
