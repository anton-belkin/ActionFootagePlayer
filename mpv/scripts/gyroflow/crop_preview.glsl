// Crop mode's preview (the gyroflow plugin's crop.lua; vo=gpu-next, gpu or libmpv): the paused frame on screen shown as
// the camera asked for sees it, until the plugin has rendered that.
//
// While paused, each turn of the camera has mpv seek to the frame again for the plugin to render it anew, which takes a
// moment. Until it's there, this moves the frame that is: a homography from the camera asked for to the one the frame
// was rendered with (both turn about the same point, so it's exact for a rectilinear view: a turn, a roll, a zoom).
// crop.lua sets it as it changes. mpv puts a new frame on screen before a script hears of it, so the mapping can't
// follow the frame from there (moving it then flashed, one or two refreshes): instead each render of the frame carries
// a parity, 0 or 1, alternating render to render, and crop.lua keeps a mapping for each. The plugin writes the parity
// into the end of the frame's last row of luma (vs-gyroflow/src/lib.rs, write_crop_mark), the first pass reads it,
// the second moves the frame with that parity's mapping and paints the mark over with the row above. A frame without
// the mark shows as it is.
//
// crop_on is 0 outside crop mode: then neither pass runs. The mappings are row-major 3x3, from the view's texture
// coordinates (0-1, y down) to the frame's.

//!PARAM crop_on
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 1
0

//!PARAM crop_a0
//!TYPE DYNAMIC float
1.0

//!PARAM crop_a1
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a2
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a3
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a4
//!TYPE DYNAMIC float
1.0

//!PARAM crop_a5
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a6
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a7
//!TYPE DYNAMIC float
0.0

//!PARAM crop_a8
//!TYPE DYNAMIC float
1.0

//!PARAM crop_b0
//!TYPE DYNAMIC float
1.0

//!PARAM crop_b1
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b2
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b3
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b4
//!TYPE DYNAMIC float
1.0

//!PARAM crop_b5
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b6
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b7
//!TYPE DYNAMIC float
0.0

//!PARAM crop_b8
//!TYPE DYNAMIC float
1.0

//!HOOK LUMA
//!BIND HOOKED
//!SAVE CROP_MARK
//!WIDTH 1
//!HEIGHT 1
//!COMPONENTS 4
//!WHEN crop_on
//!DESC Crop preview (the frame's mark)

#define MARK_PIXELS 8

// The i-th of the last row's last MARK_PIXELS pixels: one of 17 levels from black to white, as a nibble
int nibble(int i) {
    ivec2 size = ivec2(HOOKED_size);
    float v = texelFetch(HOOKED_raw, ivec2(size.x - MARK_PIXELS + i, size.y - 1), 0).x * HOOKED_mul;
    return int(clamp(floor(v * 17.0 - 0.5), 0.0, 15.0));
}

// 0: no mark; 0.5: parity 0; 1: parity 1
vec4 hook() {
    if (nibble(0) != 10 || nibble(1) != 5 || nibble(2) != 10 || nibble(3) != 5)
        return vec4(0.0);
    int high = 0;
    for (int i = 4; i < MARK_PIXELS; i++)
        high += nibble(i) > 7 ? 1 : 0;
    return vec4(high > 2 ? 1.0 : 0.5);
}

//!HOOK MAIN
//!BIND HOOKED
//!BIND CROP_MARK
//!WHEN crop_on
//!DESC Crop preview

#define MARK_PIXELS 8.0

vec4 hook() {
    vec2 size = HOOKED_size;
    vec2 uv = HOOKED_pos;
    float mark = texelFetch(CROP_MARK_raw, ivec2(0, 0), 0).x * CROP_MARK_mul;
    if (mark > 0.25) {
        vec3 q = mark > 0.75
            ? vec3(crop_b0 * uv.x + crop_b1 * uv.y + crop_b2, crop_b3 * uv.x + crop_b4 * uv.y + crop_b5, crop_b6 * uv.x + crop_b7 * uv.y + crop_b8)
            : vec3(crop_a0 * uv.x + crop_a1 * uv.y + crop_a2, crop_a3 * uv.x + crop_a4 * uv.y + crop_a5, crop_a6 * uv.x + crop_a7 * uv.y + crop_a8);
        // Behind the camera, or past the frame: what the next render will show isn't in this one
        if (q.z <= 1e-6)
            return vec4(0.0, 0.0, 0.0, 1.0);
        uv = q.xy / q.z;
        if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0))))
            return vec4(0.0, 0.0, 0.0, 1.0);
    }
    // The mark's pixels show the row above
    vec2 px = uv * size;
    if (px.y > size.y - 1.0 && px.x > size.x - MARK_PIXELS)
        px.y -= 1.0;
    return HOOKED_tex(px / size);
}
