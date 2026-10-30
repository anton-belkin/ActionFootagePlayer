// The color fringes of a flat port under water taken out, for mpv (the gyroflow plugin's; vo=gpu-next, gpu or
// libmpv): wherever the stabilization has the lens under water.
// Water bends blue more than red, so towards the edges of an underwater clip red and blue land apart from green, along
// the radius from the lens's axis: 1-2 px of a 4K frame halfway out, more further out. Here, in RGB, red and blue
// are each taken from where their ray lands with their own refractive index (vs-gyroflow/src/dispersion.rs has the
// physics and what Gyroflow's kernel does that this mirrors).
//
// Where the lens's axis is in the stabilized frame changes every frame, so the gyroflow VapourSynth plugin writes it,
// with the rest of the frame's geometry, into the start of the frame's last row of luma (Stabilize(chromatic=1)). The
// first pass reads that, once a frame, the second moves the colors and paints over it with the row above. A frame without it (not
// stabilized, or by an older plugin) shows as it is; out of the water (the lens's refraction 1) the plugin writes red
// and blue where green is. The noise reduction (shaders/denoise.glsl) may run before this: it leaves the marker as it
// is.
//
// The second pass runs for every pixel of the frame at the video's own size (4K), so it reads the marker already
// decoded, and the usual rectilinear output takes its rays without trigonometry.

//!HOOK LUMA
//!BIND HOOKED
//!SAVE CHROMATIC_MARKER
//!WIDTH 7
//!HEIGHT 1
//!COMPONENTS 4
//!DESC Chromatic aberration (the frame's marker)

#define VALUES 15
#define MAGIC 51838
#define VERSION 2

// The i-th 16-bit value: its four pixels' nibbles, each one of 17 levels from black to white
int value(int i) {
    int y = int(HOOKED_size.y) - 1;
    int v = 0;
    for (int j = 0; j < 4; j++)
        v = v * 16 + int(clamp(floor(texelFetch(HOOKED_raw, ivec2(i * 4 + j, y), 0).x * HOOKED_mul * 17.0 - 0.5), 0.0, 15.0));
    return v;
}

// The values between the magic number and the sum (1 to 13), two to a texel as their bytes over 255; all 0, which no
// version is, when the frame has no marker of this version
vec4 hook() {
    int v[VALUES];
    int sum = 0;
    for (int i = 0; i < VALUES; i++) {
        v[i] = value(i);
        if (i < VALUES - 1) sum += v[i];
    }
    if (v[0] != MAGIC || v[VALUES - 1] != sum % 65536 || (v[1] >> 8) != VERSION)
        return vec4(0.0);
    int t = int(gl_FragCoord.x) * 2 + 1;
    int a = v[t];
    int b = t + 1 < VALUES - 1 ? v[t + 1] : 0;
    return vec4(float(a >> 8), float(a & 255), float(b >> 8), float(b & 255)) / 255.0;
}

//!HOOK MAIN
//!BIND HOOKED
//!BIND CHROMATIC_MARKER
//!DESC Chromatic aberration (water)

#define VALUES 15
#define PIXELS 60.0
#define VERSION 2
// tan(1.5707): how far out the rectilinear projection reaches, as ray_radius has it
#define RECTILINEAR_MAX 10381.3

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

// The output's ray through a point of the image plane (pixels from its center, over the focal length), and back.
// Rectilinear, the same without the angles: tan(theta) is the radius
vec3 ray(vec2 n, int proj) {
    float r = length(n);
    if (r < 1e-9) return vec3(0.0, 0.0, 1.0);
    if (proj == 0) return vec3(n, 1.0) / sqrt(1.0 + r * r);
    float theta = ray_theta(r, proj);
    return vec3(n / r * sin(theta), cos(theta));
}

vec2 project(vec3 d, int proj) {
    float s = length(d.xy);
    if (s < 1e-9) return vec2(0.0);
    if (proj == 0) return d.xy / s * (d.z > 0.0 ? min(s / d.z, RECTILINEAR_MAX) : RECTILINEAR_MAX);
    return d.xy / s * ray_radius(acos(clamp(d.z / length(d), -1.0, 1.0)), proj);
}

vec4 hook() {
    vec2 size = HOOKED_size;
    vec2 px = HOOKED_pos * size;
    // As the first pass decoded them; v[0] and the sum it has checked
    int v[VALUES];
    for (int t = 0; t < 7; t++) {
        vec4 b = floor(texelFetch(CHROMATIC_MARKER_raw, ivec2(t, 0), 0) * CHROMATIC_MARKER_mul * 255.0 + 0.5);
        v[t * 2 + 1] = int(b.x) * 256 + int(b.y);
        v[t * 2 + 2] = int(b.z) * 256 + int(b.w);
    }
    if ((v[1] >> 8) != VERSION)
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
    // The project's background, where the frame doesn't reach and nothing fills it: as it is, or the picture's colors,
    // taken from nearer the axis, would spill onto it. And the picture within ~6 px of it too: its edge is blended and
    // rings into the background there, which the colors taken from further in don't have (a thin line along it). Dark
    // grey in the picture as the background is stays too, where no fringe shows anyway
    if ((v[13] & 1) == 1) {
        vec3 bg = vec3(float(v[12] >> 8), float(v[12] & 255), float(v[13] >> 8)) / 255.0;
        for (int r = 3; r <= 6; r += 3)
            for (int y = -r; y <= r; y += r)
                for (int x = -r; x <= r; x += r)
                    if (all(lessThan(abs(HOOKED_tex((px + vec2(x, y)) / size).rgb - bg), vec3(0.015))))
                        return color;
    }
    vec2 focal = vec2(float(v[4]), float(v[5])) / 2.0;
    vec2 center = 0.5 * size + vec2(signed16(v[6]), signed16(v[7])) / 4.0;
    // The axis between the sensor's first and last row (column), as the rolling shutter read them
    float t = clamp(horizontal ? px.x / size.x : px.y / size.y, 0.0, 1.0);
    vec2 a = mix(vec2(signed16(v[8]), signed16(v[9])), vec2(signed16(v[10]), signed16(v[11])), t) / 32767.0;
    vec3 axis = vec3(a, sqrt(max(1.0 - dot(a, a), 0.0)));

    // The color that lands furthest out (blue, under water) stays where it is, and the others are moved out to it: each
    // is taken from nearer the axis than where it shows. So within the picture none is ever taken from past its edge,
    // from the background or the edge fill (a colored rim along the frame's border, were they moved apart from green)
    vec3 kc = vec3(k.x, 1.0, k.y) / max(1.0, max(k.x, k.y));

    // This pixel's ray, at angle theta from the axis. A color whose index is k times green's bends there as green
    // does at the angle whose sine is k sin(theta) (Snell's law at the port): its pixel is green's ray at that angle
    vec3 d = ray((px - center) / focal, proj);
    float cos_t = clamp(dot(d, axis), -1.0, 1.0);
    vec3 side = d - cos_t * axis;
    float sin_t = length(side);
    if (sin_t < 1e-7)
        return color;
    side /= sin_t;
    for (int c = 0; c < 3; c++) {
        if (kc[c] == 1.0)
            continue;
        float s = min(kc[c] * sin_t, 1.0);
        vec2 p = center + focal * project(sqrt(1.0 - s * s) * axis + s * side, proj);
        // Never the marker's row
        p.y = min(p.y, size.y - 1.5);
        color[c] = HOOKED_tex(p / size)[c];
    }
    return color;
}
