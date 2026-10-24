// Less noise, for mpv with vo=gpu-next (the dlogm plugin's Ctrl+n; in any mpv's glsl-shaders too): each plane of
// the video blurred where it is flat and kept where it changes (a bilateral filter), before mpv turns it into RGB.
// So on log footage it works on the log signal, before the LUT lifts the shadows and their noise with them.
//
// The color planes get most of it: the blotches of color in shadows and under water are what shows of an action
// camera's noise, and the eye hardly sees color lose sharpness. The brightness gets a little, to keep the detail.
// denoise_strength scales both (glsl-shader-opts=denoise_strength=2): 0 none, 1 as tuned for 10-bit log footage
// (noise of a few code values: it halves it, and keeps 2-pixel detail and edges).
// Only YUV video has these planes: RGB video plays as it is.

//!PARAM denoise_strength
//!DESC How strongly: 0 none, 1 as tuned for 10-bit log footage, 2 twice that
//!TYPE float
//!MINIMUM 0.0
//!MAXIMUM 4.0
1.0

//!HOOK LUMA
//!BIND HOOKED
//!DESC Denoise (brightness)

// How far a neighbour counts (pixels), and how different it may be to count (of the full range): ~2x the noise of
// 10-bit log luma, well below its fine detail
#define RADIUS 1
#define SIGMA_SPACE 1.0
#define SIGMA_RANGE 0.006

vec4 hook() {
    vec4 c = HOOKED_texOff(vec2(0.0));
    float s = SIGMA_RANGE * denoise_strength;
    if (s <= 0.0)
        return c;
    vec4 sum = vec4(0.0);
    float total = 0.0;
    for (int y = -RADIUS; y <= RADIUS; y++) {
        for (int x = -RADIUS; x <= RADIUS; x++) {
            vec4 n = HOOKED_texOff(vec2(x, y));
            vec4 d = n - c;
            float w = exp(-float(x * x + y * y) / (2.0 * SIGMA_SPACE * SIGMA_SPACE) - dot(d, d) / (2.0 * s * s));
            sum += w * n;
            total += w;
        }
    }
    return sum / total;
}

//!HOOK CHROMA
//!BIND HOOKED
//!DESC Denoise (color)

// The color planes are half size (4:2:0), so 2 pixels here are 4 of the picture's. One plane with both (hardware
// decoding) or one each: either way the unused components are equal everywhere and don't count
#define RADIUS 2
#define SIGMA_SPACE 1.5
#define SIGMA_RANGE 0.012

vec4 hook() {
    vec4 c = HOOKED_texOff(vec2(0.0));
    float s = SIGMA_RANGE * denoise_strength;
    if (s <= 0.0)
        return c;
    vec4 sum = vec4(0.0);
    float total = 0.0;
    for (int y = -RADIUS; y <= RADIUS; y++) {
        for (int x = -RADIUS; x <= RADIUS; x++) {
            vec4 n = HOOKED_texOff(vec2(x, y));
            vec4 d = n - c;
            float w = exp(-float(x * x + y * y) / (2.0 * SIGMA_SPACE * SIGMA_SPACE) - dot(d, d) / (2.0 * s * s));
            sum += w * n;
            total += w;
        }
    }
    return sum / total;
}
