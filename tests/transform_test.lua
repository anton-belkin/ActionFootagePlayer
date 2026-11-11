-- Input and transform (mpv/scripts/workspace/transform.lua): bundles filled, old views split, saved or changed.
-- From the repository's folder: luajit tests/transform_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local t = require 'transform'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local session = { view = "wide", level = "strong", horizon_lock = "no", chromatic = false, aspect = "16:9",
                  projection = "stereographic", preset_horizon = "yes" }

-- An empty bundle ("stab":[], made by Moments for marks): the session's, but the frame as it played then (neutral)
local b, added = t.complete({}, session)
expect("empty: added", added, true)
expect("empty: view", b.view, "wide")
expect("empty: level", b.level, "strong")
expect("empty: horizon", b.horizon_lock, "no")
expect("empty: chromatic", b.chromatic, false)
expect("empty: aspect neutral", b.aspect, "original")
expect("empty: projection neutral", b.projection, "linear")

-- A whole bundle stays as it is
local whole = { view = "normal", level = "weak", horizon_lock = "yes", chromatic = true, aspect = "16:9", orientation = "portrait", projection = "linear", lens = "corrected", denoise_strong = false }
b, added = t.complete(whole, session)
expect("whole: nothing added", added, false)
expect("whole: aspect", b.aspect, "16:9")
expect("whole: orientation", b.orientation, "portrait")
expect("whole: chromatic kept false-able", t.complete({ chromatic = false }, { chromatic = true }).chromatic, false)

-- From before the frame: its aspect and projection neutral, not the session's
b = t.complete({ view = "normal", level = "medium", horizon_lock = "yes", chromatic = true }, session)
expect("old: aspect", b.aspect, "original")
expect("old: level kept", b.level, "medium")

-- The preset's horizon ("project") as the preset has it
b, added = t.complete({ view = "normal", level = "medium", horizon_lock = "project", chromatic = true, aspect = "original", projection = "linear" }, session)
expect("project horizon: preset's", b.horizon_lock, "yes")
expect("project horizon: added", added, true)
b = t.complete({ horizon_lock = "project" }, { horizon_lock = "no" })
expect("project horizon, no preset known: session's", b.horizon_lock, "no")

-- A value it doesn't know is replaced
b = t.complete({ level = "huge" }, session)
expect("unknown level: session's", b.level, "strong")

-- The orientation: from before it, an aspect as it showed; now the ratio landscape-wise and an orientation
b, added = t.complete({ view = "normal", level = "weak", horizon_lock = "yes", chromatic = true, aspect = "9:16", projection = "linear" }, session)
expect("old portrait: ratio landscape-wise", b.aspect, "16:9")
expect("old portrait: portrait", b.orientation, "portrait")
expect("old portrait: added", added, true)
b = t.complete({ aspect = "16:9" }, session)
expect("old landscape: stays landscape", b.orientation, "landscape")
b = t.complete({ aspect = "original" }, session)
expect("old original: as is", b.orientation, "as-is")
b, added = t.complete({ view = "normal", level = "weak", horizon_lock = "yes", chromatic = true, aspect = "4:3", orientation = "as-is", projection = "linear", lens = "corrected", denoise_strong = false }, session)
expect("new: nothing added", added, false)
expect("new: as is kept", b.orientation, "as-is")
expect("a portrait ratio as an aspect isn't one", t.valid("aspect", "3:4"), false)
-- concrete
expect("as is, landscape source", t.concrete("16:9", "as-is", 3840, 2160), "16:9")
expect("as is, portrait source", t.concrete("16:9", "as-is", 2160, 3840), "9:16")
expect("portrait of a landscape source", t.concrete("4:3", "portrait", 3840, 2160), "3:4")
expect("landscape of a portrait source", t.concrete("5:4", "landscape", 2160, 3840), "5:4")
expect("square", t.concrete("1:1", "portrait", 3840, 2160), "1:1")
expect("own, as is", t.concrete("original", "as-is", 4000, 3000), "original")
expect("own, portrait of 4:3", t.concrete("original", "portrait", 4000, 3000), "3:4")
expect("own, landscape of a landscape", t.concrete("original", "landscape", 4000, 3000), "original")
expect("own, landscape of a portrait 9:16", t.concrete("original", "landscape", 1080, 1920), "16:9")

-- The lens, before it was a key of its own: as it played (off: as recorded), an empty bundle the session's
b, added = t.complete({ view = "off", level = "weak", horizon_lock = "yes", chromatic = true, aspect = "original", orientation = "as-is", projection = "linear" }, session)
expect("old off: lens as recorded", b.lens, "as-recorded")
expect("old off: added", added, true)
b = t.complete({ view = "wide", level = "weak" }, session)
expect("old on: lens corrected", b.lens, "corrected")
b = t.complete({}, { lens = "as-recorded" })
expect("empty: the session's lens", b.lens, "as-recorded")
b = t.complete({ view = "off", lens = "corrected" }, session)
expect("off with the lens corrected: the lens alone", b.lens, "corrected")
local lensy = {}
for k, v in pairs(whole) do lensy[k] = v end
lensy.lens = "as-recorded"
expect("status: the lens changed", t.status(whole, lensy, false), "changed")
-- Over a Gyroflow project the lens and its projection are the player's too, saved with the clip (#196)
expect("status: the lens over a project is a change", t.status(whole, lensy, true), "changed")
lensy.lens, lensy.projection = "corrected", "stereographic"
expect("status: a projection over a project is a change", t.status(whole, lensy, true), "changed")
lensy.projection = "project"
expect("status: the project's own projection is a change from linear", t.status(whole, lensy, true), "changed")
b = t.complete({ view = "normal", projection = "project", lens = "as-recorded" }, session)
expect("the project's own projection is kept", b.projection, "project")
expect("…as is the lens", b.lens, "as-recorded")

-- pick, input_of
expect("pick: none", t.pick({ curve = "dlogm" }), nil)
expect("pick: level", t.pick({ curve = "dlogm", level = "weak" }).level, "weak")
expect("pick: no curve", t.pick({ curve = "dlogm", level = "weak" }).curve, nil)
expect("input_of: curve", t.input_of({ curve = "dlogm", level = "weak" }).curve, "dlogm")
expect("input_of: none", t.input_of({ level = "weak" }), nil)

-- status
expect("status none", t.status(nil, whole, false), "none")
expect("status saved", t.status(whole, whole, false), "saved")
local turned = {}
for k, v in pairs(whole) do turned[k] = v end
turned.level = "strong"
expect("status changed", t.status(whole, turned, false), "changed")
expect("status: a project's level isn't a change", t.status(whole, turned, true), "saved")
turned.view = "wide"
expect("status: a wider view over a project is a change, saved with it", t.status(whole, turned, true), "changed")
turned.view, turned.level, turned.chromatic = "normal", "weak", false
expect("status: chromatic over a project is", t.status(whole, turned, true), "changed")

-- migrate_source: input out of an old view
local doc = { view = { curve = "dlogm", view = "wide", underwater = "no", rotation = 180, aspect = "original" } }
expect("migrate: changed", t.migrate_source(doc), true)
expect("migrate: curve moved", doc.source.curve, "dlogm")
expect("migrate: water moved", doc.source.underwater, "no")
expect("migrate: turn moved", doc.source.rotation, 180)
expect("migrate: view keeps its own", doc.view.view, "wide")
expect("migrate: no curve in view", doc.view.curve, nil)
expect("migrate: no turn in view", doc.view.rotation, nil)
expect("migrate: again nothing", t.migrate_source(doc), false)

-- A turn with another aspect stays where it was, to be looked at
doc = { view = { rotation = 90, aspect = "9:16" } }
t.migrate_source(doc)
expect("migrate: portrait frame's turn stays", doc.view.rotation, 90)
expect("migrate: no source turn", doc.source, nil)

-- An input already set wins over the old view's; "project" water isn't an input
doc = { source = { curve = "hlg" }, view = { curve = "dlogm", underwater = "project", rotation = 0 } }
t.migrate_source(doc)
expect("migrate: source's curve wins", doc.source.curve, "hlg")
expect("migrate: project water dropped", doc.source.underwater, nil)
expect("migrate: emptied view removed", doc.view, nil)

-- The temporal denoise's keys of a bundle from before it was the computer's (gyroflow.conf's temporal_denoise): left out,
-- and no change
b = t.complete({ view = "wide", denoise_frames = 3, denoise_grain = 0.1, denoise_tolerance = 2.5 }, { view = "normal" })
expect("denoise: old frames left out", b.denoise_frames, nil)
expect("denoise: old grain left out", b.denoise_grain, nil)
expect("denoise: the rest kept", b.view, "wide")
expect("denoise: not picked", (t.pick({ denoise_frames = 3 })), nil)
expect("denoise: no change", t.status({ view = "wide", denoise_grain = 0.25 }, { view = "wide", denoise_grain = 0.1 }, false), "saved")

-- Stronger (denoise_strong): a clip's own kept; a bundle from before it plays without; not a boolean, without
b = t.complete({ view = "wide", denoise_strong = true }, { view = "normal", denoise_strong = false })
expect("strong: its own kept", b.denoise_strong, true)
b = t.complete({ view = "wide" }, { view = "normal", denoise_strong = true })
expect("strong: an old bundle plays without", b.denoise_strong, false)
expect("strong: not a boolean", t.complete({ denoise_strong = "yes" }, {}).denoise_strong, false)
expect("strong: a change is a change", t.status({ denoise_strong = false }, { denoise_strong = true }, false), "changed")
expect("strong: not a project's", t.status({ denoise_strong = false }, { denoise_strong = true }, true), "changed")
-- The temporal denoise on or off (denoise): a clip's own kept; a bundle from before it plays as the session does
expect("denoise: its own kept", t.complete({ view = "wide", denoise = false }, { view = "normal", denoise = true }).denoise, false)
expect("denoise: an old bundle plays as the session", t.complete({ view = "wide" }, { view = "normal", denoise = false }).denoise, false)
expect("denoise: not a boolean", t.complete({ denoise = "no" }, { denoise = true }).denoise, true)
expect("denoise: a change is a change", t.status({ denoise = true }, { denoise = false }, false), "changed")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
