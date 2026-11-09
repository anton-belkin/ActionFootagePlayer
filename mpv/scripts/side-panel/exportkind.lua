-- The Output tab's one Export button (docs/360-reframe.md, "One Export button in the Output tab"): the kinds of file
-- it makes, the rows of its list, the kind it makes now and its label. Each kind is what
-- `script-message-to gyroflow export <how>` gets: hdr (as it plays), sdr, 360, flat.
-- Plain Lua, no mpv: tests/exportkind_test.lua

local M = {}

-- What a file made as it plays is, from the dlogm plugin's state (user-data/dlogm): a word for the button and a few for
-- the list. Its looks: hdr, dv and dvsdr (HDR with Dolby Vision), tonemap (mpv's SDR on screen, HDR in the file),
-- maker and nbcu (an SDR file), off (log as recorded); a clip neither log nor HDR is SDR
function M.plays_as(d)
    if type(d) ~= "table" then return "as it plays", nil end
    if not d.log and (d.hdr or "") == "" then return "SDR", "SDR" end
    if d.look == "maker" or d.look == "nbcu" then return "SDR", "SDR" end
    if d.look == "off" then return "log", "log, as recorded" end
    if (d.look == "dv" or d.look == "dvsdr") and not d.dv_own then return "HDR", "HDR + Dolby Vision" end
    return "HDR", "HDR"
end

-- A 360 camera's own file, as the view360 plugin has it (user-data/view360): the 360° and flat exports are for it
-- alone (a stitched clip has no camera motion to export by)
function M.is360(v)
    return type(v) == "table" and v.available == true and v.stamped == true
end

-- The kind of clip a choice is remembered for
function M.clip_kind(v)
    return M.is360(v) and "360" or "flat"
end

local function recording(v)
    return type(v) == "table" and type(v.recording) == "table" and v.recording or {}
end

-- The list's rows, in order: { kind, label, disabled }. In SDR where the clip has an SDR look and isn't playing in it
-- already (it would be As it plays twice); 360° video and Flat for a 360 camera's file, Flat greyed out until there's
-- a recording
function M.rows(d, v)
    local short, long = M.plays_as(d)
    local rows = { { kind = "hdr", label = long and ("As it plays (" .. long .. ")") or "As it plays" } }
    if type(d) == "table" and (d.sdr_look or "") ~= "" and short ~= "SDR" then
        table.insert(rows, { kind = "sdr", label = "In SDR" })
    end
    if M.is360(v) then
        table.insert(rows, { kind = "360", label = "360° video" })
        local recorded = (recording(v).parts or 0) > 0
        table.insert(rows, { kind = "flat", label = recorded and "Flat, the recording" or "Flat: record a view first",
                             disabled = not recorded })
    end
    return rows
end

-- The kind the button makes: the one wanted when it's listed and can be made, else as it plays (the wish is kept by
-- the caller: In SDR comes back with the HDR look, Flat once there's a recording)
function M.chosen(rows, wanted)
    for _, r in ipairs(rows) do
        if r.kind == wanted and not r.disabled then return wanted end
    end
    return "hdr"
end

-- Why the button can't make its kind now (nil: it can)
function M.busy(kind, v)
    if (kind == "360" or kind == "flat") and recording(v).taking then return "Stop recording first (the 360° tab)" end
    return nil
end

local SUFFIX = { sdr = "SDR", ["360"] = "360°", flat = "flat" }

-- The button's label: "Export the clip · HDR", "Export the part 0:12–0:47 · flat". a, b: the part marked (ab-loop-a,
-- -b, nil when not set); clock: seconds as text
function M.label(kind, d, a, b, clock)
    local what = (a and b and b > a) and ("Export the part " .. clock(a) .. "–" .. clock(b)) or "Export the clip"
    return what .. " · " .. (SUFFIX[kind] or (M.plays_as(d)))
end

return M
