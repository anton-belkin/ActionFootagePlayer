// Underwater colour for SDR clips (GoPro and the like, Rec.709 / BT.1886): the frame's correction, then HLG in
// Rec.2020 like the log curves' shaders, so the dlogm plugin's looks (HDR, Dolby Vision, NBCU, tone mapping) take it
// from there. The correction's numbers come per frame from the dlogm plugin's uwcolor.lua (uw_estimate.lua has the
// rule); this applies them to every pixel alike:
//   red and blue moved in log-chroma (luminance kept), green leaking into red taken out in Display P3 (R - k G),
//   the strength between the camera's colour and that, the light end stretched (highlights), the lightness lifted.
// SDR white goes to HLG 75 %, as the research scripts and the player's LUT have it; what's brighter keeps its room.
//
// HOOK MAIN like the other looks: on a paused redraw libplacebo shows its cached MAIN-stage frame (dlogm_hlg.glsl.in)

//!PARAM uw_dr
//!TYPE DYNAMIC float
//!MINIMUM -3.0
//!MAXIMUM 3.0
0.0

//!PARAM uw_db
//!TYPE DYNAMIC float
//!MINIMUM -3.0
//!MAXIMUM 3.0
0.0

//!PARAM uw_k
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 1.0
0.0

//!PARAM uw_ev
//!TYPE DYNAMIC float
//!MINIMUM -2.0
//!MAXIMUM 3.0
0.0

//!PARAM uw_s
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 2.0
1.0

//!PARAM uw_hp
//!TYPE DYNAMIC float
//!MINIMUM 1.0
//!MAXIMUM 4.0
1.0

//!HOOK MAIN
//!BIND HOOKED
//!DESC underwater colour, to HLG

// Row-major matrices, applied as v * mat3(...) (GLSL's mat3 takes columns, so this is M v)
#define M709_2020 mat3(0.6274, 0.3293, 0.0433, 0.0691, 0.9195, 0.0114, 0.0164, 0.0880, 0.8956)
#define M2020_P3 mat3(1.343578253, -0.2821796705, -0.06139858206, -0.06529745279, 1.075787916, -0.01049046306, 0.002821787262, -0.01959849452, 1.016776707)
#define MP3_2020 mat3(0.7538330344, 0.1985973691, 0.04756959659, 0.04574384897, 0.9417772198, 0.01247893122, -0.001210340355, 0.0176017173, 0.9836086231)
#define LUM vec3(0.2627, 0.6780, 0.0593)
#define SDR_WHITE 0.26496256
#define EPS 4e-5

vec3 compose(float Y, float rg, float bg) {
    float er = exp(rg), eb = exp(bg);
    float g = Y / (LUM.r * er + LUM.g + LUM.b * eb);
    return vec3(g * er, g, g * eb);
}

float hlg(float e) {
    return e <= 1.0 / 12.0 ? sqrt(3.0 * e) : 0.17883277 * log(12.0 * e - 0.28466892) + 0.55991073;
}

vec4 hook() {
    vec4 c = HOOKED_texOff(0);
    // linear Rec.2020, SDR white = 1
    vec3 cam = pow(max(c.rgb, 0.0), vec3(2.4)) * M709_2020;
    float Y = dot(cam, LUM);
    vec3 l = log(max(cam, EPS));
    float rg = l.r - l.g, bg = l.b - l.g;
    vec3 o = compose(Y, rg + uw_dr, bg + uw_db);
    if (uw_k > 0.0) {
        vec3 p = o * M2020_P3;
        p.r -= uw_k * p.g;
        o = p * MP3_2020;
        o *= Y / max(dot(o, LUM), 1e-6);
    }
    if (uw_s != 1.0) {
        vec3 lo = log(max(o, EPS));
        o = compose(Y + uw_s * (dot(o, LUM) - Y), rg + uw_s * (lo.r - lo.g - rg), bg + uw_s * (lo.b - lo.g - bg));
    }
    float yo = dot(o, LUM);
    if (uw_hp != 1.0 && yo > 0.5) {
        float q = (yo - 0.5) / 0.5;
        o *= (yo + 0.5 * (uw_hp - 1.0) * (q <= 1.0 ? q * q : 2.0 * q - 1.0)) / yo;
    }
    o *= exp2(uw_ev);
    o = clamp(o * SDR_WHITE, 0.0, 1.0);
    return vec4(hlg(o.r), hlg(o.g), hlg(o.b), c.a);
}
