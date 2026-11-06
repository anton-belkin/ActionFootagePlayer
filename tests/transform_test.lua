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
local whole = { view = "normal", level = "weak", horizon_lock = "yes", chromatic = true, aspect = "9:16", projection = "linear" }
b, added = t.complete(whole, session)
expect("whole: nothing added", added, false)
expect("whole: aspect", b.aspect, "9:16")
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
expect("status: a wider view over a project isn't a change", t.status(whole, turned, true), "saved")
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

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
