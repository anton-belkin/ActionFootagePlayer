-- Underwater colour (uwcolor.lua): what a frame needs, from a small grid of it (no mpv here, so tests run it under
-- plain LuaJIT).
--
-- The rule is "v12" of research_notes/underwater_color (tile_variants.v7, chosen by Anton on 2026-11-23 from his
-- marks on ~150 GoPro frames), every step applied to the whole frame alike; the water is only measured:
--   0. f: 1 under water, fading to 0 where the frame holds real red (above water, at the surface)
--   1. red: the frame's grey-world log R/G moved to Action 6's -0.45
--   2. blue: grey-world log B/G + (0.39 - 0.32 * the camera's B/G): green frames ~+0.4, deep blue ones ~nothing
--   3. water still greenish (hue under 228 deg, when smooth water is 5 % of the frame or more): more blue, whole frame
--   4. green leaking into red, taken out in Display P3 (R - k G): the least k that makes the water no longer purple
--      (Oklab a at most -0.052) and the frame not lavender (-0.030); k at most 0.5
--   5. lightness: the frame's median luminance to Action 6's under water (stops, -1..2)
-- Units: linear Rec.2020 with SDR white at SDR_WHITE (HLG 75 %), as the research scripts have it.

local M = {}

M.SDR_WHITE = 0.26496255978640015
local LR, LG, LB = 0.2627, 0.6780, 0.0593
local M709_2020 = { 0.6274, 0.3293, 0.0433, 0.0691, 0.9195, 0.0114, 0.0164, 0.0880, 0.8956 }
local M2020_P3 = { 1.343578253, -0.2821796705, -0.06139858206, -0.06529745279, 1.075787916, -0.01049046306, 0.002821787262, -0.01959849452, 1.016776707 }
local TO709 = { 1.660511208, -0.5877105887, -0.07280061924, -0.124561406, 1.132960514, -0.008399107835, -0.01816768656, -0.100560598, 1.118728285 }
local P3i = { 0.7538330344, 0.1985973691, 0.04756959659, 0.04574384897, 0.9417772198, 0.01247893122, -0.001210340355, 0.0176017173, 0.9836086231 }
local OK1 = { 0.4122214708, 0.5363325363, 0.0514459929, 0.2119034982, 0.6806995451, 0.1073969566, 0.0883024619, 0.2817188376, 0.6299787005 }
local OK2 = { 0.2104542553, 0.7936177850, -0.0040720468, 1.9779984951, -2.4285922050, 0.4505937099, 0.0259040371, 0.7827717662, -0.8086757660 }

M.TARGET_RG = -0.45      -- Action 6's grey-world log R/G (and +0.37 B/G), linear Rec.2020 after the player's LUT
M.A_MAX = -0.052         -- water's Oklab a at most (purple side above)
M.A_FRAME = -0.030       -- the whole frame's, for frames with little water
M.K_CAP = 0.5
M.WATER_HUE_MIN = 228    -- degrees, Oklab hue of the water at least (bluer)
M.WATER_MIN = 0.05       -- share of the frame that must be smooth water before its colour steers anything
M.Y_TARGET = 0.0629      -- Action 6's median luminance under water (42 frames, the player's HLG rendering)
M.MAX_SAMPLES = 4000     -- medians are taken over at most this many cells

local function mul(m, r, g, b)
    return m[1] * r + m[2] * g + m[3] * b, m[4] * r + m[5] * g + m[6] * b, m[7] * r + m[8] * g + m[9] * b
end
M.mul = mul

local function cbrt(x) return x < 0 and -((-x) ^ (1 / 3)) or x ^ (1 / 3) end

-- Oklab of a linear Rec.709 colour: L, a, b
function M.oklab(r, g, b)
    local l, m, s = mul(OK1, r, g, b)
    return mul(OK2, cbrt(l), cbrt(m), cbrt(s))
end

local function median(t, n)
    local c = {}
    for i = 1, n do c[i] = t[i] end
    table.sort(c)
    if n == 0 then return 0 end
    if n % 2 == 1 then return c[(n + 1) / 2] end
    return (c[n / 2] + c[n / 2 + 1]) / 2
end

local function quantile(t, q)
    local c = {}
    for i = 1, #t do c[i] = t[i] end
    table.sort(c)
    if #c == 0 then return 0 end
    local pos = 1 + (#c - 1) * q
    local i = math.floor(pos)
    local fr = pos - i
    return c[i] + ((c[i + 1] or c[i]) - c[i]) * fr
end

local function clamp(x, a, b) return x < a and a or (x > b and b or x) end

-- A grid of the frame from mpv's screenshot-raw (bgr0, full-range Rec.709 / BT.1886): gw x gh cells, each the mean of
-- 3x3 samples in linear light, as linear Rec.2020 with SDR white at SDR_WHITE. Returns R, G, B arrays (1-based)
function M.grid_from_bgr0(data, w, h, stride, gw, gh)
    local lut = {}
    for q = 0, 255 do lut[q] = (q / 255) ^ 2.4 end
    local R, G, B = {}, {}, {}
    local byte = string.byte
    local i = 0
    for gy = 0, gh - 1 do
        for gx = 0, gw - 1 do
            local sr, sg, sb = 0, 0, 0
            for sy = 0, 2 do
                local y = math.floor(((gy * 3 + sy) + 0.5) * h / (gh * 3))
                local row = y * stride
                for sx = 0, 2 do
                    local x = math.floor(((gx * 3 + sx) + 0.5) * w / (gw * 3))
                    local o = row + x * 4 + 1
                    local b, g, r = byte(data, o, o + 2)
                    sr, sg, sb = sr + lut[r], sg + lut[g], sb + lut[b]
                end
            end
            i = i + 1
            local r, g, b = mul(M709_2020, sr / 9, sg / 9, sb / 9)
            R[i], G[i], B[i] = r * M.SDR_WHITE, g * M.SDR_WHITE, b * M.SDR_WHITE
        end
    end
    return R, G, B
end

-- 5x5 box mean of a gw x gh array (the border cells average what they have)
local function blur5(a, gw, gh)
    local o = {}
    for y = 0, gh - 1 do
        for x = 0, gw - 1 do
            local s, n = 0, 0
            for dy = -2, 2 do
                local yy = y + dy
                if yy >= 0 and yy < gh then
                    for dx = -2, 2 do
                        local xx = x + dx
                        if xx >= 0 and xx < gw then s, n = s + a[yy * gw + xx + 1], n + 1 end
                    end
                end
            end
            o[y * gw + x + 1] = s / n
        end
    end
    return o
end

-- 0 = smooth (open water), 1 = textured (reef, sand, things): local spread of log luminance
function M.texture(Y, gw, gh)
    local ly, sq = {}, {}
    for i = 1, gw * gh do
        local v = math.log(math.max(Y[i], 1e-4))
        ly[i], sq[i] = v, v * v
    end
    local m, m2 = blur5(ly, gw, gh), blur5(sq, gw, gh)
    local t = {}
    for i = 1, gw * gh do t[i] = clamp((math.sqrt(math.max(m2[i] - m[i] * m[i], 0)) - 0.03) / 0.07, 0, 1) end
    return blur5(t, gw, gh)
end

-- The frame's parameters and what was measured. R, G, B: the grid (linear Rec.2020, SDR white at SDR_WHITE)
function M.estimate(R, G, B, gw, gh)
    local n = gw * gh
    local Y, rg, bg = {}, {}, {}
    local sr, sg, sb, cnt = 0, 0, 0, 0
    local rgs = {}
    local Ys = {}
    local white = 0.9 * M.SDR_WHITE
    for i = 1, n do
        local r, g, b = R[i], G[i], B[i]
        local y = LR * r + LG * g + LB * b
        Y[i] = y
        Ys[#Ys + 1] = y
        local lr, lg, lb = math.log(math.max(r, 1e-5)), math.log(math.max(g, 1e-5)), math.log(math.max(b, 1e-5))
        rg[i], bg[i] = lr - lg, lb - lg
        if y > 0.002 then
            rgs[#rgs + 1] = rg[i]
            if math.max(r, g, b) < white then sr, sg, sb, cnt = sr + r, sg + g, sb + b, cnt + 1 end
        end
    end
    local out = { ok = cnt > 0 }
    if cnt == 0 then return { dr = 0, db = 0, k = 0, ev = 0, f = 0 }, out end
    local c0, c1 = math.log(sr / sg), math.log(sb / sg)
    local p95 = #rgs > 500 and quantile(rgs, 0.95) or 0
    local f = clamp((0.7 - p95) / 0.4, 0, 1)
    local t = M.texture(Y, gw, gh)
    -- the water's cells and the frame's (Y > 0.01), each thinned to at most MAX_SAMPLES for the medians
    local wi, fi = {}, {}
    for i = 1, n do
        if Y[i] > 0.01 then
            fi[#fi + 1] = i
            if t[i] < 0.15 then wi[#wi + 1] = i end
        end
    end
    local share = #wi / n
    local function thin(list)
        if #list <= M.MAX_SAMPLES then return list end
        local o, step = {}, #list / M.MAX_SAMPLES
        for j = 0, M.MAX_SAMPLES - 1 do o[#o + 1] = list[math.floor(j * step) + 1] end
        return o
    end
    wi, fi = thin(wi), thin(fi)
    local dr = (M.TARGET_RG - c0) * f
    local db = (0.39 - 0.32 * c1) * f
    -- a cell after steps 1-3, with `extra` blue
    local function compose(i, extra)
        local r2, b2 = rg[i] + dr, bg[i] + db + extra
        local er, eb = math.exp(r2), math.exp(b2)
        local gg = Y[i] / (LR * er + LG + LB * eb)
        return gg * er, gg, gg * eb
    end
    local water = f > 0 and share >= M.WATER_MIN
    -- the median colour (per channel) of `list` composed with `extra`, in Rec.709 -> Oklab hue
    local cr, cg, cb = {}, {}, {}
    local function water_hue(extra)
        for j, i in ipairs(wi) do
            local r, g, b = mul(TO709, compose(i, extra))
            cr[j], cg[j], cb[j] = r, g, b
        end
        local _, a, bb = M.oklab(median(cr, #wi), median(cg, #wi), median(cb, #wi))
        return (math.deg(math.atan2(bb, a)) + 360) % 360
    end
    local dbf, hue0 = 0, nil
    if water then
        hue0 = water_hue(0)
        if hue0 > 150 and hue0 < M.WATER_HUE_MIN then
            local lo, hi = 0, 1
            for _ = 1, 20 do
                local mid = (lo + hi) / 2
                if water_hue(mid * f) < M.WATER_HUE_MIN then lo = mid else hi = mid end
            end
            dbf = hi * f
        end
    end
    -- in Display P3: the least k with the median's Oklab a at amax
    local function p3_of(list)
        local P = { r = {}, g = {}, b = {} }
        for j, i in ipairs(list) do
            P.r[j], P.g[j], P.b[j] = mul(M2020_P3, compose(i, dbf))
        end
        P.n = #list
        P.mg, P.mb = median(P.g, P.n), median(P.b, P.n)
        return P
    end
    local qr = {}
    local function a_of(P, k)
        for j = 1, P.n do qr[j] = P.r[j] - k * P.g[j] end
        local r, g, b = mul(P3i, median(qr, P.n), P.mg, P.mb)
        local _, a = M.oklab(mul(TO709, r, g, b))
        return a
    end
    local function k_for(P, amax)
        if P.n == 0 or a_of(P, 0) <= amax then return 0 end
        local lo, hi = 0, 1
        for _ = 1, 25 do
            local mid = (lo + hi) / 2
            if a_of(P, mid) > amax then lo = mid else hi = mid end
        end
        return hi
    end
    local k, a0, a1 = 0, nil, nil
    local PW
    if f > 0 then
        if water then
            PW = p3_of(wi)
            a0 = a_of(PW, 0)
            k = k_for(PW, M.A_MAX)
        end
        k = math.max(k, k_for(p3_of(fi), M.A_FRAME))
        k = math.min(k, M.K_CAP) * f
        if PW then a1 = a_of(PW, k) end
    end
    local medY = median(Ys, #Ys)
    local ev = clamp(math.log(M.Y_TARGET / math.max(medY, 1e-4)) / math.log(2), -1, 2) * f
    out = {
        ok = true, f = f, water = share, red = dr, blue = db, blue_water = dbf, leak = k,
        centre_rg = c0, centre_bg = c1, water_hue = hue0, water_hue_after = water and water_hue(dbf) or nil,
        water_a = a0, water_a_after = a1, median_y = medY / M.SDR_WHITE, ev = ev,
    }
    return { dr = dr, db = db + dbf, k = k, ev = ev, f = f }, out
end

return M
