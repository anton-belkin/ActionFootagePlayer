-- Tags and the rating (mpv/scripts/workspace/tags.lua): tag, untag, toggle, rename and merge, what the K prompt takes,
-- the sidecar's and a moment's link's form, the recent list, and Finder's tags as a binary property list (checked
-- against plutil and xattr where they are: a Mac). From the repository's folder: luajit tests/tags_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local tags = require 'tags'
local sidecar_lib = require 'sidecar'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function j(list) return table.concat(list or {}, "|") end

-- Names: trimmed, spaces folded, compared without case, the first spelling kept
expect("clean", tags.clean("  for   family \n"), "for family")
expect("clean empty", tags.clean("   "), nil)
expect("list", j(tags.list({ "Turtle", "turtle", " reef ", 3, "" })), "Turtle|reef")
expect("add", j(tags.changed({ "Turtle" }, { "TURTLE", "reef" })), "Turtle|reef")
expect("remove any case", j(tags.changed({ "Turtle", "reef" }, nil, { "turtle" })), "reef")
local l, on = tags.toggle({ "reef" }, "Dolphin")
expect("toggle on", j(l) .. tostring(on), "reef|Dolphintrue")
l, on = tags.toggle(l, "dolphin")
expect("toggle off", j(l) .. tostring(on), "reeffalse")

-- Rename in place; to a name there already: merged; a change of case renames
local r, changed = tags.rename({ "a", "turtle", "b" }, "Turtle", "Sea turtle")
expect("rename", j(r) .. tostring(changed), "a|Sea turtle|btrue")
r, changed = tags.rename({ "turtle", "reef", "Turtles" }, "turtles", "Turtle")
expect("merge", j(r) .. tostring(changed), "turtle|reeftrue")
r, changed = tags.rename({ "turtle" }, "turtle", "Turtle")
expect("case", j(r) .. tostring(changed), "Turtletrue")
r, changed = tags.rename({ "reef" }, "turtle", "Turtle")
expect("not there", j(r) .. tostring(changed), "refalse" and "reeffalse")

-- The rating: 1-5, else none
expect("rating 0", tags.rating(0), nil)
expect("rating 7", tags.rating(7), 5)
expect("rating '3'", tags.rating("3"), 3)

-- What K takes
local p = tags.parse("Dolphin, +reef, -old,  ***, ")
expect("parse toggle", j(p.toggle), "Dolphin")
expect("parse add", j(p.add), "reef")
expect("parse remove", j(p.remove), "old")
expect("parse stars", p.rating, 3)
expect("parse *4", tags.parse("*4").rating, 4)
expect("parse 2*", tags.parse("2*").rating, 2)
expect("parse *0", tags.parse("*0").rating, 0)
expect("parse lone *", tags.parse("*").rating, 0)
expect("parse ★★", tags.parse("★★").rating, 2)
expect("parse none", tags.parse("best").rating, nil)

-- A doc changed: tags and rating left out when there are none
local doc = { tags = { "reef" }, rating = 2 }
expect("apply", tags.apply(doc, tags.parse("Turtle, reef, *5")), true)
expect("apply tags", j(doc.tags), "Turtle")
expect("apply rating", doc.rating, 5)
expect("apply same", tags.apply(doc, { add = { "turtle" } }), false)
tags.apply(doc, tags.parse("turtle, *0"))
expect("apply empty", doc.tags == nil and doc.rating == nil, true)
doc = { tags = { "Turtles", "reef" } }
expect("apply rename", tags.apply(doc, { rename = { "turtles", "Turtle" } }), true)
expect("apply rename list", j(doc.tags), "Turtle|reef")

-- Recent: most recent first, at most RECENT_MAX
local rec = tags.recent({ "a", "b", "c" }, { "B", "new" })
expect("recent", j(rec), "B|new|a|c")
local many = {}
for i = 1, 20 do many[i] = "t" .. i end
expect("recent max", #tags.recent(many, { "x" }), tags.RECENT_MAX)

-- The sidecar keeps tags and rating; a file with only tags is kept, without anything removed
local function serialize(v)
    if type(v) == "table" then
        local parts = {}
        for k, x in pairs(v) do parts[#parts + 1] = "[" .. (type(k) == "string" and string.format("%q", k) or tostring(k)) .. "]=" .. serialize(x) end
        return "{" .. table.concat(parts, ",") .. "}"
    end
    return type(v) == "string" and string.format("%q", v) or tostring(v)
end
local env = { format_json = serialize, parse_json = function(s) local f = loadstring("return " .. s); local ok, v = pcall(f or error); return ok and v or nil end }
local s = sidecar_lib.new(env)
local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local clip = dir .. "/A.MP4"
local function exists(f) local h = io.open(f, "r"); if h then h:close() end; return h ~= nil end
expect("sidecar tag", s.update(clip, function(d) return tags.apply(d, tags.parse("Turtle, **")) end), true)
local sd = s.read(clip)
expect("sidecar tags", sd and j(sd.tags), "Turtle")
expect("sidecar rating", sd and sd.rating, 2)
s.update(clip, function(d) return tags.apply(d, tags.parse("turtle")) end)
expect("sidecar rating only kept", exists(dir .. "/A.afp.json"), true)
s.update(clip, function(d) return tags.apply(d, tags.parse("*0")) end)
expect("sidecar empty removed", exists(dir .. "/A.afp.json"), false)

-- Finder's tags: names with a colour kept, hand-set ones left, removed ones taken off
local f = tags.finder_follow({ "Turtle\n6", "Mine" }, { "turtle", "reef" }, { "old" })
expect("finder follow", j(f), "Turtle\n6|Mine|reef")
expect("finder same", tags.finder_follow({ "Turtle\n6" }, { "Turtle" }, {}), nil)
f = tags.finder_follow({ "Turtle\n6", "old", "Mine" }, { "Turtle" }, { "old" })
expect("finder removed", j(f), "Turtle\n6|Mine")

-- The property list, round trip, ASCII, Cyrillic, an emoji, more than 15 entries
local names = { "Red\n6", "Привет", "🐢 turtle", "a" }
for i = 1, 16 do names[#names + 1] = "tag " .. i end
local plist = tags.finder_plist(names)
expect("plist header", plist:sub(1, 8), "bplist00")
local back = tags.finder_parse(plist)
expect("plist round trip", back and j(back), j(names))
expect("plist empty", j(tags.finder_parse(tags.finder_plist({}))), "")
expect("plist bad", tags.finder_parse("nope"), nil)
expect("hex", tags.unhex(tags.hex(plist)), plist)
expect("hex spaced", tags.unhex("62 70\n6C"), "bpl")

-- Against the system's tools, where there are (a Mac): plutil reads ours as an array of these strings, and Finder's
-- own attribute read back from xattr parses
local function sh(cmd) local h = io.popen(cmd .. " 2>/dev/null"); local o = h:read("*a"); h:close(); return o end
if sh("command -v plutil") ~= "" and sh("command -v xattr") ~= "" then
    local file = dir .. "/tags.plist"
    local h = io.open(file, "wb"); h:write(tags.finder_plist({ "Red\n6", "Привет", "reef" })); h:close()
    local json = sh("plutil -convert json -o - '" .. file .. "'")
    expect("plutil reads ours", json, '["Red\\n6","Привет","reef"]')
    -- plutil's own binary form parsed by ours
    local xml = dir .. "/x.plist"
    h = io.open(xml, "w")
    h:write('<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><array><string>Green&#10;2</string><string>Ёж</string></array></plist>')
    h:close()
    sh("plutil -convert binary1 '" .. xml .. "'")
    h = io.open(xml, "rb"); local bin = h:read("*a"); h:close()
    expect("ours reads plutil's", j(tags.finder_parse(bin)), "Green\n2|Ёж")
    -- Written as Finder's attribute and read back
    local target = dir .. "/M.mp4"
    h = io.open(target, "w"); h:write("x"); h:close()
    sh("xattr -wx " .. tags.FINDER_XATTR .. " " .. tags.hex(tags.finder_plist({ "Turtle", "Ёж" })) .. " '" .. target .. "'")
    expect("xattr round trip", j(tags.finder_parse(tags.unhex(sh("xattr -px " .. tags.FINDER_XATTR .. " '" .. target .. "'")))), "Turtle|Ёж")
    if sh("command -v mdls") ~= "" then
        -- Spotlight's view of it, where the disk is indexed (a temporary folder may not be: then it says null)
        local md = sh("mdls -raw -name kMDItemUserTags '" .. target .. "'")
        if md ~= "" and md ~= "(null)" then expect("mdls sees it", md:find("Turtle", 1, true) ~= nil, true) end
    end
end

os.execute("rm -rf '" .. dir .. "'")
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
