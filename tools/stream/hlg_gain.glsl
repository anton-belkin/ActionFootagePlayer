// The whole HLG picture made brighter on its display (afp-stream's looks with g<gain>; tools/stream/README.md).
//
// For a screen that shows HDR darker than the room wants (a TV whose HDR mode keeps to absolute nits, in a lit
// room): each pixel's display light (BT.2100 HLG, Lw 1000, system gamma 1.2) times `gain`, mid-tones and shadows as
// much as the rest, up to a knee; above it a shoulder that approaches 1000 nits, so nothing clips. Chroma ratios kept
// (one scale for R, G, B). It reads and writes HLG: right after the look's LUT (HOOK MAIN), for mpv and libplacebo.

//!PARAM gain
//!TYPE float
//!MINIMUM 1.0
//!MAXIMUM 4.0
2.0

//!HOOK MAIN
//!BIND HOOKED
//!DESC HLG made brighter

const float LW = 1000.0;
const float KNEE = 500.0;
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
    float l = LW * pow(ys, 1.2);
    float x = l * gain;
    // Linear up to the knee, then an exponential shoulder towards LW (its slope 1 at the knee)
    float l2 = x <= KNEE ? x : KNEE + (LW - KNEE) * (1.0 - exp(-(x - KNEE) / (LW - KNEE)));
    float g = l2 / l;
    float ys2 = pow(l2 / LW, 1.0 / 1.2);
    vec3 s2 = s * g * pow(ys, 0.2) / pow(ys2, 0.2);
    return vec4(hlg_oetf(s2), c.a);
}
