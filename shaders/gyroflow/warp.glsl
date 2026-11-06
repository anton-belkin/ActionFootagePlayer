// SPDX-License-Identifier: GPL-3.0-or-later
// Gyroflow's per-frame warp as an mpv/libplacebo user shader (docs/gpu-render.md, step 1: the spike). A derivative of
// gyroflow-core's kernel (src/core/stabilization/cpu_undistort.rs, gpu/wgpu_undistort.wgsl, qt_gpu/undistort.frag;
// Copyright © 2021-2022 Adrian <adrian.eddy at gmail>, GPL-3.0-or-later).
//
// This file is a template: vs-gyroflow's gyroflow-gpu-table writes the shader mpv loads, with the clip's table (each
// frame's parameters and its row rotations as quaternion knots) as a texture and the clip's constants as defines,
// where the @...@ marks are. The pass takes mpv's PTS (mpv 0.42+) and finds its frame in the table: no marks in the
// pixels. It replaces MAIN (on gpu-next the same as MAINPRESUB: after the conversion to RGB, before the looks) with a
// texture the size of the video's rectangle on screen, so it is the scaler too.
//
// What it covers so far: Gyroflow's lens models but Sony's and the generic polynomial (both need mesh data), its
// digital lenses, full lens correction, the output projections but the equirect, rolling shutter (the two-step row lookup), refraction under water, a solid background. The
// interpolation is chosen per file (INTERP): 0 bilinear, 1 B-spline (4 bilinear fetches), 2 Gyroflow's bicubic (Keys
// a = -0.75, 16 taps), 3 Lanczos4 (64 taps), 4 EWA Robidoux (Gyroflow's, up to 9x9 taps), 5 Gyroflow's bicubic from 9
// bilinear fetches. SHRINK 1 widens the
// bicubic's and Lanczos's taps where the warp shrinks the source (up to 4x, each tap a bilinear fetch).
//
// The passes (gpu_table.rs assembles them from the sections at the end of this file, each with the common code):
// - exact: one pass at MAIN, the whole geometry at every pixel;
// - grid: the geometry once a frame on a coarse grid of the output (GRID_STEP pixels apart, a compute pass into the
//   storage texture GYROFLOW_GRID), then sampling passes that interpolate it (Catmull-Rom: within 0.04 px of the
//   exact coordinates at a 32 px step): one at MAIN (RGB), or one at LUMA and one at CHROMA (the planes, so libplacebo
//   converts at the output's size, not the source's). Where a grid point has no image (past the lens's field), the
//   sampling pass falls back to the exact geometry.
//
// No directive marks (two slashes and a bang) in the comments of a pass: libplacebo takes them as directives anywhere.

//!PARAM PTS
//!TYPE DYNAMIC float
0.0

//!PARAM gyroflow_gpu_on
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 1
1

//!TEXTURE GYROFLOW_TABLE
//!SIZE @TABLE_W@ @TABLE_H@
//!FORMAT rgba32f
//!FILTER NEAREST
@TABLE_HEX@

@PASSES@

// ==== common ====

// The table: frame i's block of BLOCK texels at (i % COLS) * BLOCK, row i / COLS. Texels 0 to PARAM_TEXELS - 1 the
// frame's parameters, then KNOTS quaternions (x, y, z, w) of the row rotations, the knots evenly over the rows
// (FULL_ROWS: one per row)
//   0: f.x, f.y, c.x, c.y            1-6: the lens's coefficients k[0..24]
//   7: fov, lens correction, refraction coefficient, field limit
//   8: translation2d.x, .y, matrix rows (1: no rolling shutter), valid (1)
//   9: translation3d.x, .y, .z, 0    10-13: the digital lens's parameters [0..16]
//  14: input horizontal stretch, vertical stretch, red's and blue's refractive index over green's (1 out of the water)

#define PI 3.14159265358979
#define PARAM_TEXELS 15

int table_frame;
// 1 for green (Gyroflow's own coefficient); red's or blue's ratio to it while the chromatic coordinates are found
float refr_scale = 1.0;
vec4 fc, lens, t2, stretch;
vec3 t3;

// What Gyroflow's lens functions (shaders/gyroflow/lens, its src/core/stabilization/distortion_models/*.glsl) read,
// named as they name it
struct GyroflowParams {
    vec4 k1, k2, k3, k4, k5, k6;
    vec4 digital_lens_params[4];
    float width, height, output_width, output_height;
    int flags;
};
GyroflowParams params;

vec4 table(int t)
{
    int col = table_frame % COLS;
    return texelFetch(GYROFLOW_TABLE, ivec2(col * BLOCK + t, table_frame / COLS), 0);
}

vec4 qmul(vec4 a, vec4 b)
{
    return vec4(a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz), a.w * b.w - dot(a.xyz, b.xyz));
}

vec3 qrot(vec4 q, vec3 v)
{
    vec3 t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// The rotation of matrix row y (0 .. rows - 1): Gyroflow's matrix of that row, from the knots
vec4 row_rotation(int y, float rows)
{
#if FULL_ROWS
    return table(PARAM_TEXELS + y);
#else
    float t = rows > 1.0 ? float(y) / (rows - 1.0) * float(KNOTS - 1) : 0.0;
    int j = min(int(t), KNOTS - 2);
    vec4 q = mix(table(PARAM_TEXELS + j), table(PARAM_TEXELS + 1 + j), t - float(j));
    return q / length(q);
#endif
}

// projection::unproject, the projections but the equirect: output plane point -> ray (an angle vector)
vec2 unproject(vec2 n)
{
    float r = length(n);
    if (r < 1e-12) return vec2(0.0);
    float theta;
#if PROJ == 1
    theta = 2.0 * atan(r * 0.5);
#elif PROJ == 2
    theta = r;
#elif PROJ == 3
    theta = 2.0 * asin(min(r * 0.5, 1.0));
#elif PROJ == 4
    theta = asin(min(r, 1.0));
#else
    theta = atan(r);
#endif
    return n * (theta / r);
}

vec3 ray_to_dir(vec2 ray)
{
    float theta = length(ray);
    if (theta < 1e-12) return vec3(ray, 1.0);
    return vec3(ray * (sin(theta) / theta), cos(theta));
}

// The clip's lens model and digital lens, Gyroflow's GLSL, put in here by gpu_table.rs: distort_point(x, y, z) and
// undistort_point, and with a digital lens (HAS_DIGITAL) digital_distort_point and digital_undistort_point
@LENS@

// Stabilization::rotate_and_distort: a ray in the output camera, turned into the source camera by row rotation q,
// imaged by the lens. False where the ray has no image (past the field limit, outside Snell's window)
// want_boundary: where the ray has no image (past the lens's field, outside Snell's window), uv is still found, where the
// ray at the edge of what the lens sees lands (boundary_uv set), for the edge fill to take its colors from
bool want_boundary = false;
bool boundary_uv = false;
bool rotate_and_distort(vec3 d, vec4 q, out vec2 uv)
{
    vec3 p = qrot(q, d) + t3;
    float rxy = length(p.xy);
    float theta = atan(rxy, p.z);
    uv = vec2(0.0);
    boundary_uv = false;
    bool fail = false;
    if (lens.w > 0.0 && theta > lens.w) {
        if (!want_boundary || rxy < 1e-12) return false;
        fail = true;
        theta = lens.w;
        p = vec3(p.xy * (sin(theta) / rxy), cos(theta));
        rxy = length(p.xy);
    }
    // Under water, the channel's own index (chromatic: red's and blue's over green's, refr_scale)
    float refr = lens.z * refr_scale;
    if (refr != 1.0 && refr > 0.0 && rxy > 1e-12) {
        float sin_d = sin(theta) * refr;
        if (sin_d >= 1.0 || p.z <= 0.0) {
            if (!want_boundary) return false;
            fail = true;
            sin_d = 0.9999;
        }
        float theta_d = asin(sin_d);
        p = vec3(p.xy * (sin(theta_d) / rxy), cos(theta_d));
    }
    uv = distort_point(p.x, p.y, p.z) * fc.xy + fc.zw;
#if HAS_DIGITAL
    uv = digital_distort_point(uv);
#endif
    if (stretch.x > 0.001) uv.x /= stretch.x;
    if (stretch.y > 0.001) uv.y /= stretch.y;
    // The models' "no image" (-99999, from their Newton solves): no image here either
    bool image = uv.x > -9999.0 && uv.y > -9999.0;
    boundary_uv = fail && image;
    return image && !fail;
}

#ifndef GRID_PASS
// The sampling passes only (the grid pass binds no HOOKED, and a compute shader has no dFdx)
// ---- Sampling the source; uv in its pixels, centers at whole numbers ----

vec4 src(vec2 p) { return HOOKED_tex((p + 0.5) * HOOKED_pt); }

float cubic(float x)
{
    // Keys, a = -0.75: OpenCV's and Gyroflow's bicubic
    const float a = -0.75;
    x = abs(x);
    if (x < 1.0) return ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0;
    if (x < 2.0) return ((a * x - 5.0 * a) * x + 8.0 * a) * x - 4.0 * a;
    return 0.0;
}

float lanczos4(float x)
{
    x = abs(x);
    if (x < 1e-5) return 1.0;
    if (x >= 4.0) return 0.0;
    float px = PI * x;
    return 4.0 * sin(px) * sin(px * 0.25) / (px * px);
}

#if INTERP == 1
vec4 sample_src(vec2 uv)
{
    // Cubic B-spline through 4 bilinear fetches (softer than Keys)
    vec2 c = floor(uv);
    vec2 f = uv - c;
    vec2 f2 = f * f, f3 = f2 * f;
    vec2 w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    vec2 w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    vec2 w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    vec2 w3 = f3 / 6.0;
    vec2 s0 = w0 + w1, s1 = w2 + w3;
    vec2 p0 = c - 1.0 + w1 / s0, p1 = c + 1.0 + w3 / s1;
    return s0.y * (s0.x * src(vec2(p0.x, p0.y)) + s1.x * src(vec2(p1.x, p0.y)))
         + s1.y * (s0.x * src(vec2(p0.x, p1.y)) + s1.x * src(vec2(p1.x, p1.y)));
}
#elif INTERP == 2 || INTERP == 3
#if INTERP == 2
#define KERNEL cubic
#define RADIUS 2.0
#else
#define KERNEL lanczos4
#define RADIUS 4.0
#endif
vec4 sample_src(vec2 uv, vec2 scale)
{
#if SHRINK
    // Widened where the warp shrinks the source: the kernel scale.x by scale.y pixels wide, taps half that apart
    // (each a bilinear fetch), as view360.glsl does
    vec2 step = vec2(scale.x > 2.0 ? scale.x * 0.5 : 1.0, scale.y > 2.0 ? scale.y * 0.5 : 1.0);
    vec2 base = vec2(step.x == 1.0 ? floor(uv.x) : uv.x, step.y == 1.0 ? floor(uv.y) : uv.y);
    ivec2 lo = ivec2(ceil((uv - RADIUS * scale - base) / step));
    ivec2 hi = ivec2(floor((uv + RADIUS * scale - base) / step));
#else
    scale = vec2(1.0);
    vec2 step = vec2(1.0);
    vec2 base = floor(uv);
    ivec2 lo = ivec2(1.0 - RADIUS), hi = ivec2(RADIUS);
#endif
    vec4 sum = vec4(0.0);
    float wsum = 0.0;
    for (int j = lo.y; j <= hi.y; j++) {
        float ty = base.y + float(j) * step.y;
        float wy = KERNEL((ty - uv.y) / scale.y);
        for (int i = lo.x; i <= hi.x; i++) {
            float tx = base.x + float(i) * step.x;
            float w = wy * KERNEL((tx - uv.x) / scale.x);
            sum += w * src(vec2(tx, ty));
            wsum += w;
        }
    }
    return sum / wsum;
}
#elif INTERP == 4
// Gyroflow's EWA (Robidoux): an ellipse covering a unit circle in both the source and the output, cubic BC weights
// over its radius. jac = (du/dx, du/dy, dv/dx, dv/dy)
#define EWA_MAX 4.0
float bc2(float x)
{
    const float B = 0.3782157, C = 0.3108921;
    x = abs(x);
    float x2 = x * x;
    if (x < 1.0) return ((6.0 - 2.0 * B) + (-18.0 + 12.0 * B + 6.0 * C) * x2 + (12.0 - 9.0 * B - 6.0 * C) * x2 * x) / 6.0;
    if (x < 2.0) return ((8.0 * B + 24.0 * C) + (-12.0 * B - 48.0 * C) * x + (6.0 * B + 30.0 * C) * x2 + (-B - 6.0 * C) * x2 * x) / 6.0;
    return 0.0;
}
vec4 sample_src(vec2 uv, vec4 jac)
{
    vec2 box = min(2.0 * max(vec2(max(abs(jac.x + jac.y), abs(jac.x - jac.y)), max(abs(jac.z + jac.w), abs(jac.z - jac.w))), 1.0), 2.0 * EWA_MAX);
    float f0 = abs(jac.x * jac.w - jac.y * jac.z);
    float f = max(f0 * f0, 0.1);
    float A = (jac.z * jac.z + jac.w * jac.w) / f;
    float B = -2.0 * (jac.x * jac.z + jac.y * jac.w) / f;
    float C = (jac.x * jac.x + jac.y * jac.y) / f;
    vec2 v = vec2(C - A, -B);
    float lv = length(v);
    float v0 = lv > 0.01 ? v.x / lv : 1.0;
    float cc = sqrt(max(1.0 + v0, 0.0) / 2.0);
    float s = sqrt(max(1.0 - v0, 0.0) / 2.0);
    float a0 = A * cc * cc - B * cc * s + C * s * s;
    float c0 = A * s * s + B * cc * s + C * cc * cc;
    float bt1 = B * (cc * cc - s * s), bt2 = 2.0 * (A - C) * cc * s;
    float b0 = bt1 + bt2, b0v2 = bt1 - bt2;
    if (abs(b0) > abs(b0v2)) { s = -s; b0 = b0v2; }
    a0 = min(a0, 1.0);
    c0 = min(c0, 1.0);
    float sn = -s;
    vec3 abc = vec3(a0 * cc * cc - b0 * cc * sn + c0 * sn * sn,
                    2.0 * a0 * cc * sn + b0 * cc * cc - b0 * sn * sn - 2.0 * c0 * cc * sn,
                    a0 * sn * sn + b0 * cc * sn + c0 * cc * cc);
    ivec4 bounds = ivec4(floor(uv.x - box.x), ceil(uv.x + box.x), floor(uv.y - box.y), ceil(uv.y + box.y));
    vec4 sum = vec4(0.0);
    float wsum = 0.0;
    for (int y = bounds.z; y <= bounds.w; y++) {
        float fy = float(y) - uv.y;
        for (int x = bounds.x; x <= bounds.y; x++) {
            float fx = float(x) - uv.x;
            float k = bc2(sqrt(fx * fx * abc.x + fx * fy * abc.y + fy * fy * abc.z));
            if (k == 0.0) continue;
            sum += k * src(vec2(x, y));
            wsum += k;
        }
    }
    return wsum != 0.0 ? sum / wsum : src(uv);
}
#elif INTERP == 5
vec4 sample_src(vec2 uv)
{
    // Gyroflow's bicubic (Keys, a = -0.75) from 9 bilinear fetches: per axis the two middle taps (both weights positive)
    // become one fetch between them, the outer two (negative) stay, so 3 x 3 fetches instead of 16 taps
    vec2 c = floor(uv);
    vec2 f = uv - c;
    vec2 w0 = vec2(cubic(1.0 + f.x), cubic(1.0 + f.y));
    vec2 w1 = vec2(cubic(f.x), cubic(f.y));
    vec2 w2 = vec2(cubic(1.0 - f.x), cubic(1.0 - f.y));
    vec2 w3 = vec2(cubic(2.0 - f.x), cubic(2.0 - f.y));
    vec2 w12 = w1 + w2;
    vec2 p0 = c - 1.0, p12 = c + w2 / w12, p3 = c + 2.0;
    vec4 r0 = w0.x * src(vec2(p0.x, p0.y)) + w12.x * src(vec2(p12.x, p0.y)) + w3.x * src(vec2(p3.x, p0.y));
    vec4 r1 = w0.x * src(vec2(p0.x, p12.y)) + w12.x * src(vec2(p12.x, p12.y)) + w3.x * src(vec2(p3.x, p12.y));
    vec4 r2 = w0.x * src(vec2(p0.x, p3.y)) + w12.x * src(vec2(p12.x, p3.y)) + w3.x * src(vec2(p3.x, p3.y));
    return w0.y * r0 + w12.y * r1 + w3.y * r2;
}
#else
vec4 sample_src(vec2 uv) { return src(uv); }
#endif

#endif

// The frame's parameters, from the table by PTS. False where the table has none
bool frame_setup()
{
    // A moment played as a video of its own starts TIME_OFFSET seconds into the clip
    table_frame = clamp(int(floor((PTS + TIME_OFFSET) * FPS + 0.5)), 0, FRAMES - 1);
    fc = table(0);
    params.k1 = table(1); params.k2 = table(2); params.k3 = table(3);
    params.k4 = table(4); params.k5 = table(5); params.k6 = table(6);
    lens = table(7);
    t2 = table(8);
    t3 = table(9).xyz;
    for (int i = 0; i < 4; i++) params.digital_lens_params[i] = table(10 + i);
    stretch = table(14);
    params.width = float(SRC_W); params.height = float(SRC_H);
    params.output_width = float(OUT_W); params.output_height = float(OUT_H);
    params.flags = HAS_DIGITAL * 2;
    return t2.w >= 0.5;
}

// Output pixel j (0 .. target_size - 1, pixel centers at whole numbers) -> its source coordinate in Gyroflow's frame
// (SRC_W x SRC_H, centers at whole numbers): Stabilization::undistort_coord. False where the ray has no image. The
// output pixel goes into the kernel's output frame (OUT_W x OUT_H) as the VapourSynth path maps its pixel j there:
// j * OUT_W / width
bool source_coord(vec2 j, out vec2 uv)
{
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_pos = j * size / target_size + t2.xy;
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / lens.x;
    vec3 d = ray_to_dir(unproject((out_pos - 0.5 * size) / out_f));
    // The rolling shutter's row: where the middle row's rotation puts the ray, then that row's rotation
    float rows = t2.z;
    int idx = 0;
    if (rows > 1.0) {
        int mid = int(rows) / 2;
        if (rotate_and_distort(d, row_rotation(mid, rows), uv))
            idx = clamp(int(floor(uv.y + 0.5)), 0, SRC_H);
        idx = min(idx, int(rows) - 1);
    }
    return rotate_and_distort(d, row_rotation(idx, rows), uv);
}

#ifndef GRID_PASS
// The picture at source coordinate uv (Gyroflow's frame); past the frame by more than half a pixel, the background, as
// Gyroflow's taps outside it are
bool outside(vec2 uv) { return any(lessThan(uv, vec2(-0.5))) || any(greaterThan(uv, vec2(SRC_W, SRC_H) - 0.5)); }

// Gyroflow's frame -> the texture's pixels (the same frame unless mpv hands over another size)
vec2 texel(vec2 uv)
{
    uv = (uv + 0.5) * HOOKED_size / vec2(SRC_W, SRC_H) - 0.5;
#ifdef PLANE_OFFSET
    // A plane read on its own: where its samples sit against the frame's, in its pixels (MPEG-2's left-sited chroma:
    // +0.25 across)
    uv += PLANE_OFFSET;
#endif
    return uv;
}

// The edges the frame doesn't cover (EDGE_FILL 1, the plugin's "smooth"): the frame's own colors where its edge is
// nearest, blurred over EDGE_BLUR of its height and muted with the distance (less color, a little darker), so the
// stabilized frame's motion doesn't show there as a hard line. The sphere trail isn't on the GPU yet: the same fill
vec4 edge_fill(vec2 uv)
{
    vec2 edge = clamp(uv, vec2(0.0), vec2(SRC_W, SRC_H) - 1.0);
    // Wider further out, so the frame's edge doesn't streak across the fill
    float r = max(EDGE_BLUR * float(SRC_H), 1.0) * (1.0 + 6.0 * clamp(length(uv - edge) / float(SRC_H) * 4.0, 0.0, 1.0));
    vec4 sum = vec4(0.0);
    for (int y = -2; y <= 2; y++)
        for (int x = -2; x <= 2; x++)
            sum += src(texel(clamp(edge + vec2(x, y) * (0.5 * r), vec2(0.0), vec2(SRC_W, SRC_H) - 1.0)));
    sum /= 25.0;
    float m = clamp(length(uv - edge) / float(SRC_H) * 4.0, 0.0, 1.0);
#ifdef PLANE_CHROMA
    return mix(sum, vec4(0.5), 0.6 * m);
#elif defined(PLANE_LUMA)
    return sum * (1.0 - 0.25 * m);
#else
    float l = dot(sum.rgb, vec3(0.2627, 0.678, 0.0593));
    return vec4(mix(sum.rgb, vec3(l), 0.6 * m) * (1.0 - 0.25 * m), sum.a);
#endif
}

vec4 finish(vec2 uv)
{
    if (outside(uv)) {
#ifdef UV_OUT
        return BACKGROUND;
#elif EDGE_FILL
        return edge_fill(uv);
#else
        return BACKGROUND;
#endif
    }
#ifdef UV_OUT
    // The tests' output (gpu_table.rs): the source coordinate, not the picture
    return vec4(uv, 0.0, 1.0);
#else
    uv = texel(uv);
#if INTERP == 4
    vec2 ux = dFdx(uv), uy = dFdy(uv);
    return sample_src(uv, vec4(ux.x, uy.x, ux.y, uy.y));
#elif INTERP == 2 || INTERP == 3
    vec2 ux = dFdx(uv), uy = dFdy(uv);
    return sample_src(uv, clamp(max(abs(ux), abs(uy)), 1.0, 4.0));
#else
    return sample_src(uv);
#endif
#endif
}


#if CHROMATIC
// Under water: each channel read at its own coordinate (red and blue refracted by their own index, rb: red's xy,
// blue's xy), once, from the source; red and blue bilinear (CHROMATIC_FAST) or with the main kernel
vec4 finish_rgb(vec2 uv, vec4 rb)
{
    vec4 c = finish(uv);
    if (outside(uv) || outside(rb.xy) || outside(rb.zw)) return c;
#ifdef UV_OUT
    return c;
#else
#if CHROMATIC_FAST
    c.r = src(texel(rb.xy)).r;
    c.b = src(texel(rb.zw)).b;
#else
    c.r = finish(rb.xy).r;
    c.b = finish(rb.zw).b;
#endif
    return c;
#endif
}
#endif
#endif

#ifdef GRID_STEP
// The grid: point (i, g) at output pixel ((i, g) - 1) * GRID_STEP, one before the edge to two past it
ivec2 grid_size() { return ivec2(ceil(target_size / float(GRID_STEP))) + 3; }

float cr0(float t) { return ((-0.5 * t + 1.0) * t - 0.5) * t; }
float cr1(float t) { return (1.5 * t - 2.5) * t * t + 1.0; }
float cr2(float t) { return ((-1.5 * t + 2.0) * t + 0.5) * t; }
float cr3(float t) { return (0.5 * t - 0.5) * t * t; }

#ifndef GRID_PASS
// Output pixel j's source coordinate from the grid (Catmull-Rom over 4x4 points); false where one of them has no image
// Red's and blue's coordinates (xy, zw) where the grid pass marked the frame under water (a point's z is 2: grid_wet)
vec4 grid_rb;
bool grid_wet;
bool grid_coord(vec2 j, out vec2 uv)
{
    grid_rb = vec4(0.0);
    grid_wet = false;
    vec2 g = j / float(GRID_STEP) + 1.0;
    ivec2 i = ivec2(floor(g));
    vec2 f = g - vec2(i);
    ivec2 hi = grid_size() - 1;
#if GRID_LINEAR
    // Bilinear over 2x2 points: 4 reads instead of 16 (within 0.06 px at an 8 px step)
    vec4 p00 = imageLoad(GYROFLOW_GRID, clamp(i, ivec2(0), hi)), p10 = imageLoad(GYROFLOW_GRID, clamp(i + ivec2(1, 0), ivec2(0), hi));
    vec4 p01 = imageLoad(GYROFLOW_GRID, clamp(i + ivec2(0, 1), ivec2(0), hi)), p11 = imageLoad(GYROFLOW_GRID, clamp(i + ivec2(1, 1), ivec2(0), hi));
    uv = mix(mix(p00.xy, p10.xy, f.x), mix(p01.xy, p11.xy, f.x), f.y);
#if CHROMATIC
    grid_wet = p00.z > 1.5;
    if (grid_wet) {
        vec4 q00 = imageLoad(GYROFLOW_GRID_RB, clamp(i, ivec2(0), hi)), q10 = imageLoad(GYROFLOW_GRID_RB, clamp(i + ivec2(1, 0), ivec2(0), hi));
        vec4 q01 = imageLoad(GYROFLOW_GRID_RB, clamp(i + ivec2(0, 1), ivec2(0), hi)), q11 = imageLoad(GYROFLOW_GRID_RB, clamp(i + ivec2(1, 1), ivec2(0), hi));
        grid_rb = mix(mix(q00, q10, f.x), mix(q01, q11, f.x), f.y);
    }
#endif
    return min(min(p00.z, p10.z), min(p01.z, p11.z)) > 0.5;
#else
    vec4 wx = vec4(cr0(f.x), cr1(f.x), cr2(f.x), cr3(f.x));
    vec4 wy = vec4(cr0(f.y), cr1(f.y), cr2(f.y), cr3(f.y));
    vec2 sum = vec2(0.0);
    float valid = 1.0;
    for (int y = 0; y < 4; y++) {
        vec2 row = vec2(0.0);
        for (int x = 0; x < 4; x++) {
            vec4 p = imageLoad(GYROFLOW_GRID, clamp(i + ivec2(x - 1, y - 1), ivec2(0), hi));
            row += wx[x] * p.xy;
            valid = min(valid, p.z);
#if CHROMATIC
            if (x == 1 && y == 1) grid_wet = p.z > 1.5;
            grid_rb += wx[x] * wy[y] * imageLoad(GYROFLOW_GRID_RB, clamp(i + ivec2(x - 1, y - 1), ivec2(0), hi));
#endif
        }
        sum += wy[y] * row;
    }
    uv = sum;
    return valid > 0.5;
#endif
}
#endif
#endif

// ==== pass exact ====
vec4 hook()
{
    if (!frame_setup()) return BACKGROUND;
    vec2 j = HOOKED_pos * target_size - 0.5;
    vec2 uv;
    want_boundary = EDGE_FILL == 1;
    if (!source_coord(j, uv)) {
#if EDGE_FILL && !defined(UV_OUT)
        if (boundary_uv) return edge_fill(uv);
#endif
        return BACKGROUND;
    }
    want_boundary = false;
#if CHROMATIC
    if (stretch.z != 1.0 || stretch.w != 1.0) {
        vec4 rb = vec4(uv, uv);
        vec2 c;
        refr_scale = stretch.z;
        if (source_coord(j, c)) rb.xy = c;
        refr_scale = stretch.w;
        if (source_coord(j, c)) rb.zw = c;
        return finish_rgb(uv, rb);
    }
#endif
    return finish(uv);
}

// ==== pass grid ====
void hook()
{
    ivec2 g = ivec2(gl_GlobalInvocationID.xy);
    if (any(greaterThanEqual(g, grid_size()))) return;
    vec2 uv = vec2(0.0);
    vec2 j = vec2(g - 1) * float(GRID_STEP);
    bool ok = frame_setup() && source_coord(j, uv);
#if CHROMATIC
    bool wet = ok && (stretch.z != 1.0 || stretch.w != 1.0);
    imageStore(GYROFLOW_GRID, g, vec4(uv, ok ? (wet ? 2.0 : 1.0) : 0.0, 0.0));
    // Red's and blue's coordinates, where the frame is under water
    vec4 rb = vec4(uv, uv);
    if (wet) {
        vec2 c;
        refr_scale = stretch.z;
        if (source_coord(j, c)) rb.xy = c;
        refr_scale = stretch.w;
        if (source_coord(j, c)) rb.zw = c;
    }
    imageStore(GYROFLOW_GRID_RB, g, rb);
#else
    imageStore(GYROFLOW_GRID, g, vec4(uv, ok ? 1.0 : 0.0, 0.0));
#endif
}

// ==== pass sample ====
vec4 hook()
{
    vec2 j = HOOKED_pos * target_size - 0.5;
    vec2 uv;
    // The frame's parameters only where the grid has no image (each is a fetch per pixel). Not one condition with &&:
    // naga evaluates both calls, and the second overwrites uv
    bool ok = grid_coord(j, uv);
#ifndef GRID_STRICT
    // The tests leave this out: the grid alone. Where the ray has no image, the edge fill from where the lens's field ends
    if (!ok) {
        want_boundary = EDGE_FILL == 1;
        ok = frame_setup() && source_coord(j, uv);
#if EDGE_FILL
        if (!ok && boundary_uv) return edge_fill(uv);
#endif
    }
#endif
    if (!ok) return BACKGROUND;
#if CHROMATIC
    if (grid_wet) return finish_rgb(uv, grid_rb);
#endif
    return finish(uv);
}
