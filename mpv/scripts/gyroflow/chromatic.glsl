// The color fringes of a flat port under water taken out, for mpv (the gyroflow plugin's Ctrl+a; vo=gpu-next, gpu or
// libmpv).
// Water bends blue more than red, so towards the edges of an underwater clip red and blue land apart from green, along
// the radius from the lens's axis: 1-2 px of a 4K frame halfway out, more further out. Here, in RGB, red and blue
// are each taken from where their ray lands with their own refractive index (vs-gyroflow/src/dispersion.rs has the
// physics and what Gyroflow's kernel does that this mirrors).
//
// Where the lens's axis is in the stabilized frame changes every frame, so the gyroflow VapourSynth plugin writes it,
// with the rest of the frame's geometry, into the start of the frame's last row of luma (Stabilize(chromatic=1)). The
// first pass reads that, the second moves the colors and paints over it with the row above. A frame without it (not
// stabilized, or by an older plugin) shows as it is; out of the water, and with Ctrl+a off, the plugin writes red and
// blue where green is. The noise reduction (shaders/denoise.glsl) may run before this: it leaves the marker as it is.

//!HOOK LUMA
//!BIND HOOKED
//!SAVE CHROMATIC_MARKER
//!WIDTH 13
//!HEIGHT 1
//!COMPONENTS 4
//!DESC Chromatic aberration (the frame's marker)

// One texel per 16-bit value: its four pixels' nibbles, over 15. A nibble is one of 17 levels from black to white
vec4 hook() {
    int x = int(gl_FragCoord.x) * 4;
    int y = int(HOOKED_size.y) - 1;
    vec4 n;
    for (int j = 0; j < 4; j++)
        n[j] = clamp(floor(texelFetch(HOOKED_raw, ivec2(x + j, y), 0).x * HOOKED_mul * 17.0 - 0.5), 0.0, 15.0);
    return n / 15.0;
}

//!HOOK MAIN
//!BIND HOOKED
//!BIND CHROMATIC_MARKER
//!DESC Chromatic aberration (water)

#define VALUES 13
#define PIXELS 52.0
#define MAGIC 51838

int marker(int i) {
    vec4 n = floor(texelFetch(CHROMATIC_MARKER_raw, ivec2(i, 0), 0) * CHROMATIC_MARKER_mul * 15.0 + 0.5);
    return int(n.x) * 4096 + int(n.y) * 256 + int(n.z) * 16 + int(n.w);
}

float signed16(int v) {
    return float(v >= 32768 ? v - 65536 : v);
}

// Gyroflow's output projections (ray_unproject, ray_radius in its wgpu_undistort.wgsl): 0 rectilinear,
// 1 stereographic, 2 equidistant, 3 equisolid angle, 4 orthographic
float ray_theta(float r, int proj) {
    if (proj == 1) return 2.0 * atan(r * 0.5);
    if (proj == 2) return r;
    if (proj == 3) return 2.0 * asin(min(r * 0.5, 1.0));
    if (proj == 4) return asin(min(r, 1.0));
    return atan(r);
}

float ray_radius(float theta, int proj) {
    if (proj == 1) return 2.0 * tan(theta * 0.5);
    if (proj == 2) return theta;
    if (proj == 3) return 2.0 * sin(theta * 0.5);
    if (proj == 4) return sin(theta);
    return tan(min(theta, 1.5707));
}

// The output's ray through a point of the image plane (pixels from its center, over the focal length), and back
vec3 ray(vec2 n, int proj) {
    float r = length(n);
    if (r < 1e-9) return vec3(0.0, 0.0, 1.0);
    float theta = ray_theta(r, proj);
    return vec3(n / r * sin(theta), cos(theta));
}

vec2 project(vec3 d, int proj) {
    float s = length(d.xy);
    if (s < 1e-9) return vec2(0.0);
    return d.xy / s * ray_radius(acos(clamp(d.z / length(d), -1.0, 1.0)), proj);
}

vec4 hook() {
    vec2 size = HOOKED_size;
    vec2 px = HOOKED_pos * size;
    int v[VALUES];
    int sum = 0;
    for (int i = 0; i < VALUES; i++) {
        v[i] = marker(i);
        if (i < VALUES - 1) sum += v[i];
    }
    if (v[0] != MAGIC || v[VALUES - 1] != sum % 65536 || (v[1] >> 8) != 1)
        return HOOKED_texOff(vec2(0.0));

    // The marker's pixels show the row above
    if (px.y > size.y - 1.0 && px.x < PIXELS)
        px.y -= 1.0;
    vec4 color = HOOKED_tex(px / size);

    int proj = v[1] & 15;
    bool horizontal = ((v[1] >> 4) & 1) == 1;
    vec2 k = 1.0 + vec2(signed16(v[2]), signed16(v[3])) / 2097152.0;
    if (k == vec2(1.0))
        return color;
    vec2 focal = vec2(float(v[4]), float(v[5])) / 2.0;
    vec2 center = 0.5 * size + vec2(signed16(v[6]), signed16(v[7])) / 4.0;
    // The axis between the sensor's first and last row (column), as the rolling shutter read them
    float t = clamp(horizontal ? px.x / size.x : px.y / size.y, 0.0, 1.0);
    vec2 a = mix(vec2(signed16(v[8]), signed16(v[9])), vec2(signed16(v[10]), signed16(v[11])), t) / 32767.0;
    vec3 axis = vec3(a, sqrt(max(1.0 - dot(a, a), 0.0)));

    // This pixel's ray, at angle theta from the axis. A color whose index is k times green's bends there as green
    // does at the angle whose sine is k sin(theta) (Snell's law at the port): its pixel is green's ray at that angle
    vec3 d = ray((px - center) / focal, proj);
    float cos_t = clamp(dot(d, axis), -1.0, 1.0);
    vec3 side = d - cos_t * axis;
    float sin_t = length(side);
    if (sin_t < 1e-7)
        return color;
    side /= sin_t;
    for (int c = 0; c < 2; c++) {
        float s = min(k[c] * sin_t, 1.0);
        vec2 p = center + focal * project(sqrt(1.0 - s * s) * axis + s * side, proj);
        // Never the marker's row
        p.y = min(p.y, size.y - 1.5);
        float value = HOOKED_tex(p / size)[c == 0 ? 0 : 2];
        if (c == 0) color.r = value; else color.b = value;
    }
    return color;
}
