// HLG highlights expanded into the display's headroom (afp-stream's looks with x<peak>; tools/stream/README.md).
//
// The D-Log M -> HLG LUT (dlogm_hlg.glsl) puts the camera's brightest code at HLG 0.867, 422 nits on BT.2100's
// 1000-nit display: the sun and glints never go above it, and HDR's headroom up to 1000 nits stays unused. Here the
// display light of each pixel (BT.2100 HLG, Lw 1000, system gamma 1.2) is scaled by 1 + a * (L / 422)^2 on its
// luminance L, a = peak / 422 - 1: 422 nits go to `peak`, 200 to ~260, 100 to ~108 (for 1000), mid grey (~26)
// stays. The chroma ratios are kept (the scale is the same for R, G, B); a channel past the signal's top clips.
// It reads and writes HLG, so it goes right after the look's LUT (HOOK MAIN, as dlogm_hlg.glsl), for mpv and
// libplacebo alike.

//!PARAM peak
//!TYPE float
//!MINIMUM 422.0
//!MAXIMUM 1000.0
1000.0

//!HOOK MAIN
//!BIND HOOKED
//!DESC HLG highlights into the headroom

const float LW = 1000.0;
const float CEILING = 422.0;
const float HA = 0.17883277;
const float HB = 0.28466892;
const float HC = 0.55991073;

vec3 hlg_inverse_oetf(vec3 e) {
    e = clamp(e, 0.0, 1.0);
    vec3 lo = e * e / 3.0;
    vec3 hi = (exp((e - HC) / HA) + HB) / 12.0;
    return mix(lo, hi, step(0.5, e));
}

vec3 hlg_oetf(vec3 s) {
    s = clamp(s, 0.0, 1.0);
    vec3 lo = sqrt(3.0 * s);
    vec3 hi = HA * log(max(12.0 * s - HB, 1e-6)) + HC;
    return mix(lo, hi, step(1.0 / 12.0, s));
}

vec4 hook() {
    vec4 c = HOOKED_tex(HOOKED_pos);
    vec3 s = hlg_inverse_oetf(c.rgb);
    float ys = dot(s, vec3(0.2627, 0.6780, 0.0593));
    if (ys <= 0.0)
        return c;
    float l = LW * pow(ys, 1.2);                       // display luminance, nits
    float a = peak / CEILING - 1.0;
    float g = 1.0 + a * (l / CEILING) * (l / CEILING);  // the scale of the display light
    // Back to scene light: display = LW * Ys^0.2 * s, and the new Ys from the new luminance
    float ys2 = pow(l * g / LW, 1.0 / 1.2);
    vec3 s2 = s * g * pow(ys, 0.2) / pow(ys2, 0.2);
    return vec4(hlg_oetf(s2), c.a);
}
