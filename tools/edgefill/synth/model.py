# The sphere's rule (warp.glsl's "pass sphere") done the plain way, for the synthetic bench: every texel of the
# sphere the output shows, each frame, from its own direction (no pixel lattice, no ownership, no compute passes), and
# the output read from it. Where the shader and this differ, look at the shader; where this looks wrong too, the rule
# itself is wrong. The shader measures the sectors as it writes, so its strengths are a frame later than these.
import math
import numpy as np
from scipy import ndimage
import synth as S

# The shader's rule as warp.glsl has it (v4, 2026-11-19). v3's: SW=2048 SOFT=0.10 SOFT_IN=0.005 MEMORY=v3 REACH=falloff
DEFAULTS = dict(SW=512, BAND=0.03, FULL=0.08, FALLOFF=0.30, CHROMA=2.0, LO=0.04, HI=0.10, RISE=0.15, FALL=0.3,
                SLOW=0.5, TAKE=0.08, SOFT=0.012, SOFT_IN=0.004, SECTORS=32, FEATHER=0.03, INSET=0.002, BLUR=0.02,
                SOFT_RULE='max', SOFT_T=0.25, MEMORY='split', RING=0.0, RING_FADE=0, RING_GATE=0, REACH='antipode')
# MEMORY: what the sectors compare the frame with. 'v3': the slow layer, which continuation writes also move (so a
# shaking frame's edge, past which the continuation keeps being written, keeps finding its memory 'different').
# 'split': texels the camera has seen keep what it saw (continuation writes don't touch their memory), and only a
# change there (the world changed) rewrites traces; texels that only ever held a continuation are compared on their
# own, and only they are rewritten when the frame shows the continuation was wrong
# RING (Anton 2026-11-19): a band this share of the height around the frame, past its edge, always takes this frame's
# continuation when the frame is written (RING_FADE=1: wholly at the edge, fading to nothing at the band's far side);
# what it covers then holds a continuation, not a trace. RING_GATE=1: the band is as wide as the sector's difference
# says (RING x strength), so where the frame's edge matches what the sphere held there is no band at all
# REACH (Anton 2026-11-19): how far a sector's continuation goes. 'falloff': FULL..FALLOFF of the height past the edge.
# 'antipode': the whole wedge of the sphere around the frame's axis, out to the point behind it, the parts the output
# doesn't show too (a trace is worth something only while it is joined to the frame; past a changed edge it isn't)
# SOFT_RULE: how the frame's soft edge goes into the sphere. 'max' (v3): each texel keeps the largest share it has had
# and only gets the rest. 'converge': while the frame covers a texel, it moves towards the frame at a rate its share
# sets (half of the way in SOFT_T / share seconds), so a still frame's edge ends up the frame, and a moving one leaves
# a soft trace


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def edge_distance(a, b, chroma):
    d = a - b
    y = d @ np.array([0.2126, 0.7152, 0.0722])
    return np.sqrt(y ** 2 + (chroma * (d[..., 2] - y) / 1.8556) ** 2 + (chroma * (d[..., 0] - y) / 1.5748) ** 2)


LO = np.array([-0.5, -0.5])
HI = np.array([S.SRC_W - 0.5, S.SRC_H - 0.5])


def edge_place(uv):
    """0..1 clockwise from the top left corner along the frame's edge (the nearest place), and the depth inside"""
    e = np.clip(uv, LO, HI)
    dl, dr = e[..., 0] - LO[0], HI[0] - e[..., 0]
    dt, db = e[..., 1] - LO[1], HI[1] - e[..., 1]
    w, h = float(S.SRC_W), float(S.SRC_H)
    in_d = np.minimum(np.minimum(dl, dr), np.minimum(dt, db))
    p = np.where(in_d == dt, dl, np.where(in_d == dr, w + dt, np.where(in_d == db, w + h + dr, 2 * w + h + db)))
    return p / (2 * (w + h)), in_d


class Model:
    def __init__(self, W, H, **kw):
        self.p = dict(DEFAULTS, **kw)
        self.W, self.H = W, H
        sw = self.p['SW']
        self.SW, self.SH = sw, sw // 2
        self.C = np.zeros((self.SH, self.SW, 3))
        self.K = np.zeros((self.SH, self.SW), bool)
        self.Sl = np.zeros((self.SH, self.SW, 3))
        self.Wsh = np.zeros((self.SH, self.SW))
        self.Sn = np.zeros((self.SH, self.SW), bool)
        self.dmc = np.zeros(self.p['SECTORS'])
        self.dm = np.zeros(self.p['SECTORS'])
        self.first = True
        IX, IY = np.meshgrid(np.arange(self.SW), np.arange(self.SH))
        self.D = S.lonlat_dir((IX + 0.5) / self.SW * 2 * np.pi - np.pi, (IY + 0.5) / self.SH * np.pi - np.pi / 2)
        # the output pixels' rays (output_ray: out_pos = j * OUT / RT, rectilinear)
        jx, jy = np.meshgrid(np.arange(W), np.arange(H))
        n = np.stack([(jx * S.SRC_W / W - S.SRC_W / 2) / S.OUT_F, (jy * S.SRC_H / H - S.SRC_H / 2) / S.OUT_F], -1)
        d = np.concatenate([n, np.ones(n.shape[:2] + (1,))], -1)
        self.d_out = d / np.linalg.norm(d, axis=-1, keepdims=True)

    # ---- the frame and its continuation ----
    def _px(self, uv, shape):
        h, w = shape[:2]
        return (uv[..., 0] + 0.5) * w / S.SRC_W - 0.5, (uv[..., 1] + 0.5) * h / S.SRC_H - 0.5

    def _sample(self, img, uv, order=1):
        x, y = self._px(uv, img.shape)
        return np.stack([ndimage.map_coordinates(img[..., c], [y, x], order=order, mode='nearest') for c in range(3)], -1)

    def _prepare(self, frame, qc):
        """The smooth fill as the shader's fill passes make it (warp.glsl fill_lo, fill_pull, fill_push, fill_blur):
        a 135-cell-high grid over the output, each cell the frame's average at 3 x 3 places (its edge repeated past it)
        and how much the frame covers; pulled down and pushed up; blurred over EDGE_BLUR of the height"""
        self.frame = frame
        W, H = self.W, self.H
        fh = 135
        fw = int(round(fh * S.SRC_W / S.SRC_H))
        cx, cy = np.meshgrid(np.arange(fw), np.arange(fh))
        acc = np.zeros((fh, fw, 3))
        cov = np.zeros((fh, fw))
        for t in range(9):
            jx = (cx + (t % 3 + 0.5) / 3) * W / fw - 0.5
            jy = (cy + (t // 3 + 0.5) / 3) * H / fh - 0.5
            d = self._ray(jx, jy)
            uv = self._source(qc, S.qrot(self.qv, d))
            acc += self._sample(frame, np.clip(uv, 0, [S.SRC_W - 1, S.SRC_H - 1]))
            cov += self.feather(uv)
        levels = [np.concatenate([acc / 9, (cov / 9)[..., None]], -1)]
        while levels[-1].shape[0] > 1 or levels[-1].shape[1] > 1:
            f = levels[-1]
            h2, w2 = (f.shape[0] + 1) // 2, (f.shape[1] + 1) // 2
            pad = np.zeros((h2 * 2, w2 * 2, 4))
            pad[:f.shape[0], :f.shape[1]] = f
            ws = pad[..., 3]
            rgb = (pad[..., :3] * ws[..., None])
            tot = ws[0::2, 0::2] + ws[1::2, 0::2] + ws[0::2, 1::2] + ws[1::2, 1::2]
            c = rgb[0::2, 0::2] + rgb[1::2, 0::2] + rgb[0::2, 1::2] + rgb[1::2, 1::2]
            c = np.where(tot[..., None] > 1e-6, c / np.maximum(tot, 1e-6)[..., None], 0)
            levels.append(np.concatenate([c, np.minimum(tot, 1)[..., None]], -1))
        up = levels[-1]
        for k in range(len(levels) - 2, -1, -1):
            f = levels[k]
            cs = up.shape[:2]
            yy, xx = np.meshgrid(np.arange(f.shape[0]), np.arange(f.shape[1]), indexing='ij')
            py = np.clip((yy + 0.5) * 0.5, 0.5, cs[0] - 0.5) - 0.5
            px = np.clip((xx + 0.5) * 0.5, 0.5, cs[1] - 0.5) - 0.5
            co = np.stack([ndimage.map_coordinates(up[..., c], [py, px], order=1, mode='nearest') for c in range(3)], -1)
            w = f[..., 3:4]
            rgb = np.where(w >= 1, f[..., :3], f[..., :3] * w + co * (1 - w))
            up = np.concatenate([rgb, np.ones(w.shape)], -1)
        sigma = max(self.p['BLUR'] * fh, 0.3)
        r = min(int(math.ceil(3 * sigma)), 24)
        k = np.exp(-np.arange(-r, r + 1) ** 2 / (2 * sigma * sigma))
        k /= k.sum()
        g = ndimage.correlate1d(up[..., :3], k, axis=1, mode='nearest')
        self.fill = ndimage.correlate1d(g, k, axis=0, mode='nearest')
        self.fw, self.fh = fw, fh

    def cont(self, jx, jy):
        """The smooth fill at output place j (the shader's edge_fill: FILL_RAW at cur_j, bilinear; unmuted)"""
        gx = (jx + 0.5) / self.W * self.fw - 0.5
        gy = (jy + 0.5) / self.H * self.fh - 0.5
        return np.stack([ndimage.map_coordinates(self.fill[..., c], [gy, gx], order=1, mode='nearest') for c in range(3)], -1)

    def _ray(self, jx, jy):
        n = np.stack([(jx * S.SRC_W / self.W - S.SRC_W / 2) / S.OUT_F, (jy * S.SRC_H / self.H - S.SRC_H / 2) / S.OUT_F], -1)
        d = np.concatenate([n, np.ones(n.shape[:-1] + (1,))], -1)
        return d / np.linalg.norm(d, axis=-1, keepdims=True)

    # ---- geometry ----
    def _source(self, qc, d_w):
        d = S.qrot(S.qconj(qc), d_w)
        z = d[..., 2]
        ok = z > 1e-3
        zz = np.where(ok, z, 1e-3)
        uv = np.stack([S.F * d[..., 0] / zz + S.SRC_W / 2, S.F * d[..., 1] / zz + S.SRC_H / 2], -1)
        uv[~ok] = 1e7
        return uv

    def _cells(self, qv):
        """The sphere's texels the output shows (with a texel's margin): indices and directions"""
        fwd = S.qrot(qv, np.array([0.0, 0.0, 1.0]))
        lon0, lat0 = S.lonlat(fwd)
        span_lon, span_lat = math.radians(80), math.radians(60)
        ix0 = int(((lon0 - span_lon) / (2 * np.pi) + 0.5) * self.SW)
        ix1 = int(((lon0 + span_lon) / (2 * np.pi) + 0.5) * self.SW) + 1
        iy0 = max(int(((lat0 - span_lat) / np.pi + 0.5) * self.SH), 0)
        iy1 = min(int(((lat0 + span_lat) / np.pi + 0.5) * self.SH) + 1, self.SH)
        ix = np.arange(ix0, ix1) % self.SW
        iy = np.arange(iy0, iy1)
        IX, IY = np.meshgrid(ix, iy)
        lon = (IX + 0.5) / self.SW * 2 * np.pi - np.pi
        lat = (IY + 0.5) / self.SH * np.pi - np.pi / 2
        d_w = S.lonlat_dir(lon, lat)
        d_o = S.qrot(S.qconj(qv), d_w)
        z = np.where(d_o[..., 2] > 1e-3, d_o[..., 2], 1e-3)
        jx = (S.OUT_F * d_o[..., 0] / z + S.SRC_W / 2) * self.W / S.SRC_W
        jy = (S.OUT_F * d_o[..., 1] / z + S.SRC_H / 2) * self.H / S.SRC_H
        vis = (d_o[..., 2] > 1e-3) & (jx > -2) & (jx < self.W + 1) & (jy > -2) & (jy < self.H + 1)
        return IY[vis], IX[vis], d_w[vis], jx[vis], jy[vis]

    def strength(self, place, s):
        n = len(s)
        x = place * n - 0.5
        c = np.floor(x)
        a = (c.astype(int) + n) % n
        b = (a + 1) % n
        return s[a] * (1 - (x - c)) + s[b] * (x - c)

    # ---- one frame ----
    def step(self, frame, qv, qc, dt=1 / S.FPS):
        p = self.p
        Hs = float(S.SRC_H)
        self.qv = qv
        self._prepare(frame, qc)
        iy, ix, d_w, jx, jy = self._cells(qv)
        uv = self._source(qc, d_w)
        place, in_d = edge_place(uv)
        e = np.clip(uv, LO, HI)
        out_d = np.linalg.norm(uv - e, axis=-1)
        inside = out_d <= 0
        known = self.K[iy, ix]
        # a coarser sphere than the shader's takes the frame averaged over a texel (no aliasing)
        if self.SW != 2048:
            sig = 0.5 * (2 * np.pi / self.SW) * S.F * frame.shape[1] / S.SRC_W
            ff = ndimage.gaussian_filter(frame, (sig, sig, 0), mode='nearest')
        else:
            ff = frame
        f = self._sample(ff, np.where(inside[..., None], uv, e))
        # sectors: the frame now against the memory of the same directions, in the band inside the edge
        nsec = p['SECTORS']
        sec = np.minimum(np.floor(place * nsec), nsec - 1).astype(int)
        band = inside & (in_d <= p['BAND'] * Hs)
        n_in = np.bincount(sec[band], minlength=nsec)
        has = n_in >= 4
        split = p['MEMORY'] == 'split'
        seen = self.Sn[iy, ix] & known

        def measure(sel):
            n_k = np.bincount(sec[sel], minlength=nsec)
            e_sum = np.stack([np.bincount(sec[sel], f[sel, c], nsec) for c in range(3)], -1)
            m_sum = np.stack([np.bincount(sec[sel], self.Sl[iy[sel], ix[sel], c], nsec) for c in range(3)], -1)
            dmem = np.where(n_k > 0, edge_distance(e_sum / np.maximum(n_k, 1)[:, None], m_sum / np.maximum(n_k, 1)[:, None], p['CHROMA'])
                            * np.clip(3 * n_k / np.maximum(n_in, 1), 0, 1), 0.0)
            return np.where(has, dmem, -1.0)

        def smooth(dm, d_now):
            if self.first:
                return np.zeros_like(dm)
            hl = np.where(d_now > dm, p['RISE'], p['FALL'])
            return np.where(d_now >= 0, dm + (d_now - dm) * (1 - np.exp2(-dt / hl)), dm - dm * (1 - np.exp2(-dt / p['FALL'])))

        self.dm = smooth(self.dm, measure(band & (seen if split else known)))
        self.dmc = smooth(self.dmc, measure(band & known & ~seen)) if split else self.dmc
        self.first = False
        self.s = smoothstep(p['LO'], p['HI'], self.dm)
        self.sc = smoothstep(p['LO'], p['HI'], self.dmc)
        # writes
        slow_k = 1 - np.exp2(-dt / p['SLOW'])
        take = 1 - np.exp2(-dt / p['TAKE'])
        C, Sl, Wsh = self.C[iy, ix], self.Sl[iy, ix], self.Wsh[iy, ix]
        cont = self.cont(jx, jy)
        # inside: the frame with a soft edge, only the share the texel hasn't had
        a_in = smoothstep(p['SOFT_IN'] * Hs + 1.5, p['SOFT'] * Hs, in_d)
        w = np.where(known, Wsh, 0.0)
        if p['SOFT_RULE'] == 'converge':
            share = np.where(a_in >= 0.999, 1.0, 1 - np.exp2(-dt * a_in / p['SOFT_T']))
        else:
            share = np.where(a_in >= 0.999, 1.0, np.where(a_in > w, (a_in - w) / np.maximum(1 - w, 1e-6), 0.0))
        under = np.where(known[..., None], C, cont)
        c_in = under + (f - under) * share[..., None]
        had = seen if split else known
        slow_in = np.where((a_in > 0)[..., None], np.where(had[..., None], Sl + (f - Sl) * slow_k, f), np.where(known[..., None], Sl, c_in))
        # outside: the continuation, wholly where empty, else the sector's share
        st = self.strength(place, self.s)
        if split:
            st = np.where(seen, st, np.maximum(st, self.strength(place, self.sc)))
        reach = 1.0 if p['REACH'] == 'antipode' else 1 - smoothstep(p['FULL'] * Hs, p['FALLOFF'] * Hs, out_d)
        a_out = np.where(known, take * st * reach, 1.0)
        a_out = np.where(a_out < 1 / 512, 0.0, a_out)
        c_out = np.where(known[..., None], C + (cont - C) * a_out[..., None], cont)
        slow_out = np.where(known[..., None], Sl + (cont - Sl) * a_out[..., None], cont)
        if split:
            slow_out = np.where(seen[..., None], Sl, slow_out)
        ring = np.zeros_like(out_d)
        if p['RING'] > 0:
            rb = p['RING'] * Hs * (st if p['RING_GATE'] else 1.0)
            ring = np.where(out_d <= rb, 1 - out_d / np.maximum(rb, 1e-6) if p['RING_FADE'] else 1.0, 0.0)
            ring = np.where(inside, 0.0, ring)
            c_out = c_out + (cont - c_out) * ring[..., None]
            slow_out = slow_out + (cont - slow_out) * ring[..., None]
        self.C[iy, ix] = np.where(inside[..., None], c_in, c_out)
        self.Sl[iy, ix] = np.where(inside[..., None], slow_in, slow_out)
        self.Wsh[iy, ix] = np.where(inside, np.maximum(w, a_in), w * (1 - a_out))
        self.K[iy, ix] = True
        if p['REACH'] == 'antipode':
            self._beyond(qc, iy, ix, take, split)
        self.Sn[iy, ix] = (seen & (ring < 0.5)) | (inside & (a_in > 0))
        return self.render(frame, qv, qc)

    def _wedge(self, qc, d_w):
        """Each direction's angle around the frame's axis, and the place on the frame's edge that angle points to"""
        d = S.qrot(S.qconj(qc), d_w)
        phi = np.arctan2(d[..., 1], d[..., 0])
        c, s_ = np.cos(phi), np.sin(phi)
        hw, hh = S.SRC_W / 2, S.SRC_H / 2
        t = np.minimum(hw / np.maximum(np.abs(c), 1e-9), hh / np.maximum(np.abs(s_), 1e-9)) * 0.999
        place, _ = edge_place(np.stack([hw + t * c, hh + t * s_], -1))
        return phi, place

    def _beyond(self, qc, iy, ix, take, split):
        """REACH antipode: the sphere the output doesn't show takes its wedge's continuation at the sector's strength;
        the colour is the continuation where that wedge leaves the output (its border, averaged per angle)"""
        W, H = self.W, self.H
        bx = np.concatenate([np.arange(0, W, 4), np.full(H // 4, W - 1), np.arange(0, W, 4), np.zeros(H // 4)]).astype(float)
        by = np.concatenate([np.zeros(W // 4 + (W % 4 > 0)), np.arange(0, H, 4), np.full(W // 4 + (W % 4 > 0), H - 1), np.arange(0, H, 4)]).astype(float)
        bphi, _ = self._wedge(qc, S.qrot(self.qv, self._ray(bx, by)))
        bc = self.cont(bx, by)
        nb = 64
        bin_ = ((bphi + np.pi) / (2 * np.pi) * nb).astype(int) % nb
        n = np.bincount(bin_, minlength=nb)
        col = np.stack([np.bincount(bin_, bc[:, c], nb) for c in range(3)], -1)
        have = n > 0
        col = np.where(have[:, None], col / np.maximum(n, 1)[:, None], 0)
        idx = np.arange(nb)
        for c in range(3):   # bins the border never reaches: around the circle from the nearest that do
            col[~have, c] = np.interp(idx[~have], idx[have], col[have, c], period=nb)
        mask = self.K.copy()
        mask[iy, ix] = False
        D = self.D[mask]
        phi, place = self._wedge(qc, D)
        x = (phi + np.pi) / (2 * np.pi) * nb - 0.5
        c0 = np.floor(x).astype(int)
        fr = (x - c0)[:, None]
        cc = col[c0 % nb] * (1 - fr) + col[(c0 + 1) % nb] * fr
        st = self.strength(place, self.s)
        seen = self.Sn[mask]
        if split:
            st = np.where(seen, st, np.maximum(st, self.strength(place, self.sc)))
        a = take * st
        a = np.where(a < 1 / 512, 0.0, a)[:, None]
        self.C[mask] = self.C[mask] + (cc - self.C[mask]) * a
        sl = self.Sl[mask] + (cc - self.Sl[mask]) * a
        self.Sl[mask] = np.where(seen[:, None], self.Sl[mask], sl) if split else sl

    # ---- the output ----
    def feather(self, uv):
        d = np.minimum(uv + 0.5, np.array([S.SRC_W - 0.5, S.SRC_H - 0.5]) - uv)
        return np.clip((d.min(-1) - self.p['INSET'] * S.SRC_H) / (self.p['FEATHER'] * S.SRC_H), 0, 1)

    def render(self, frame, qv, qc):
        d_w = S.qrot(qv, self.d_out)
        uv = self._source(qc, d_w)
        m = self.feather(uv)
        need = m < 1
        out = np.zeros((self.H, self.W, 3))
        lon, lat = S.lonlat(d_w[need])
        tx = (lon / (2 * np.pi) + 0.5) * self.SW - 0.5
        ty = (lat / np.pi + 0.5) * self.SH - 0.5
        K = self.K.astype(float)
        ks = ndimage.map_coordinates(K, [ty, tx], order=3, prefilter=False, mode='grid-wrap')
        cs = np.stack([ndimage.map_coordinates(self.C[..., c] * K, [ty, tx], order=3, prefilter=False, mode='grid-wrap') for c in range(3)], -1)
        jy_, jx_ = np.nonzero(need)
        base = self.cont(jx_.astype(float), jy_.astype(float))
        fill = base + (cs / np.maximum(ks, 1e-6)[..., None] - base) * np.clip(ks * 1.5, 0, 1)[..., None]
        out[need] = fill
        inside = m > 0
        fr = self._sample(frame, uv[inside])
        out[inside] = out[inside] * (1 - m[inside])[..., None] + fr * m[inside][..., None]
        self.uv, self.m = uv, m
        return out

    def truth(self, qv, which, gain):
        lon, lat = S.lonlat(S.qrot(qv, self.d_out))
        return S.world(lon, lat, which, gain)
