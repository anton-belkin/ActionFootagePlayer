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

//!PARAM gyroflow_cam_x
//!TYPE DYNAMIC float
0.0

//!PARAM gyroflow_cam_y
//!TYPE DYNAMIC float
0.0

//!PARAM gyroflow_cam_z
//!TYPE DYNAMIC float
0.0

//!PARAM gyroflow_cam_w
//!TYPE DYNAMIC float
1.0

//!PARAM gyroflow_cam_zoom
//!TYPE DYNAMIC float
1.0

//!PARAM gyroflow_dn_after
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 5
5

//!PARAM gyroflow_dn_epoch
//!TYPE DYNAMIC int
0

//!PARAM gyroflow_dn_radius
//!TYPE DYNAMIC int
//!MINIMUM 1
//!MAXIMUM 5
5

//!PARAM gyroflow_dn_keep
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 1.0
0.25

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
//   9: translation3d.x, .y, .z, the clip's frame count    10-13: the digital lens's parameters [0..16]
//  14: input horizontal stretch, vertical stretch, red's and blue's refractive index over green's (1 out of the water)
//  15: the output camera in the world (a quaternion: the sphere trail's directions)

#define PI 3.14159265358979
#define PARAM_TEXELS 16

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

// A 360 camera's sphere (pass sphere360) has a block of the table per lens: lens_base is where the lens's begins
int lens_base = 0;

vec4 table(int t)
{
    int col = table_frame % COLS;
    return texelFetch(GYROFLOW_TABLE, ivec2(col * BLOCK + lens_base + t, table_frame / COLS), 0);
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

#if !defined(GRID_PASS) && !defined(SPHERE_PASS) && !defined(DN_COMPUTE)
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
    // The frame count is frame 0's texel 9's w
    int frames = int(texelFetch(GYROFLOW_TABLE, ivec2(9, 0), 0).w);
    table_frame = clamp(int(floor((PTS + TIME_OFFSET) * FPS + 0.5)), 0, max(frames - 1, 0));
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
// Output pixel j's ray, in the output camera. Crop mode's camera (main.lua's gyroflow_cam_ parameters, set as it turns:
// no new table) turns it first, and zooms by the FOV: gyroflow-core's view_rotation multiplies each row's rotation on
// the right by the view's quaternion in its matrices' axes (gpu_table::camera_quat), the world's too, and its zoom is
// the FOV's (Camera::apply)
vec3 output_ray(vec2 j)
{
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_pos = j * size / target_size + t2.xy;
#if PROJ == 5
    // The equirect (a 360 camera's sphere): its plane is the sphere's, all of it 360 x 180 degrees at zoom (fov) 1;
    // longitude across, latitude down, +z at the middle, +x at +90 degrees, up (-y) at the top (the fork's
    // equirect_to_dir)
    vec2 n = (out_pos - 0.5 * size) / (vec2(float(OUT_W) / 6.2831853, float(OUT_H) / 3.1415927) / max(lens.x, 1e-6));
    float cl = cos(n.y);
    vec3 d = vec3(cl * sin(n.x), sin(n.y), cl * cos(n.x));
#else
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * gyroflow_cam_zoom);
    vec3 d = ray_to_dir(unproject((out_pos - 0.5 * size) / out_f));
#endif
    return qrot(vec4(gyroflow_cam_x, gyroflow_cam_y, gyroflow_cam_z, gyroflow_cam_w), d);
}

bool source_coord(vec2 j, out vec2 uv)
{
    vec3 d = output_ray(j);
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

#if !defined(GRID_PASS) && !defined(SPHERE_PASS) && !defined(DN_COMPUTE)
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

// The output pixel being drawn (the sphere trail's direction, the fill's place)
vec2 cur_j;

// The picture's share at source coordinate uv: 1 inside, easing to 0 over FILL_FEATHER of the height at the frame's
// edge, from FILL_INSET in (edges.rs's mask: the interpolation's taps past the edge pull in the background), so the
// picture fades into the fill instead of meeting it in a line
#define FILL_FEATHER 0.03
#define FILL_INSET 0.002
float feather(vec2 uv)
{
    vec2 d = min(uv + 0.5, vec2(SRC_W, SRC_H) - 0.5 - uv);
    return clamp((min(d.x, d.y) - FILL_INSET * float(SRC_H)) / max(FILL_FEATHER * float(SRC_H), 1.0), 0.0, 1.0);
}

#if !defined(DN_PASS) && !defined(FILL_PASS)
// The edges the frame doesn't cover (EDGE_FILL 1, the plugin's "smooth"; edges.rs, smooth.rs): the fill passes' grid
// (GYROFLOW_FILL: the frame's colors spread out from where it is, blurred over EDGE_BLUR of the height) at this output
// pixel, muted with the distance from the frame (less color and a little darker, easing in over FILL_MUTE_RAMP of the
// height), so it doesn't draw the eye from the picture
#define FILL_MUTE_RAMP 0.12
vec4 edge_fill(vec2 uv)
{
#if EDGE_FILL && !defined(UV_OUT)
    vec4 f = texture(FILL_RAW, (cur_j + 0.5) / target_size) * FILL_MUL;
    vec2 edge = clamp(uv, vec2(-0.5), vec2(SRC_W, SRC_H) - 0.5);
    float t = clamp(length(uv - edge) / (FILL_MUTE_RAMP * float(SRC_H)), 0.0, 1.0);
    float m = t * t * (3.0 - 2.0 * t);
    float sat = 1.0 - 0.35 * m, bright = 1.0 - 0.12 * m;
#ifdef PLANE_CHROMA
    return vec4(0.5 + (f.xy - 0.5) * sat, 0.0, 1.0);
#elif defined(PLANE_LUMA)
    return vec4(vec3(0.0625 + (f.x - 0.0625) * bright), 1.0);
#else
    float mean = (f.r + f.g + f.b) / 3.0;
    return vec4((mean + (f.rgb - mean) * sat) * bright, 1.0);
#endif
#else
    return BACKGROUND;
#endif
}
#endif

#if !defined(DN_PASS) && !defined(FILL_PASS)
#if SPHERE_FILL
ivec2 sphere_texel(vec3 w)
{
    vec2 size = vec2(imageSize(GYROFLOW_SPHERE));
    float lon = atan(w.x, w.z), lat = asin(clamp(w.y / length(w), -1.0, 1.0));
    return ivec2(floor(vec2(lon / (2.0 * PI) + 0.5, lat / PI + 0.5) * size));
}

// The sphere trail (the plugin's sphere): what was last seen in this pixel's direction of the world, fading over a few
// seconds (a half-life of 2.5 s, gone after 10), over the smooth fill; where nothing was seen, the smooth fill
vec4 sphere_fill(vec2 uv)
{
    vec4 base = edge_fill(uv);
    if (!frame_setup()) return base;
    ivec2 t = sphere_texel(qrot(table(15), output_ray(cur_j)));
    ivec2 size = imageSize(GYROFLOW_SPHERE);
    vec3 sum = vec3(0.0);
    float ws = 0.0;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            ivec2 p = ivec2((t.x + 2 * x + size.x) % size.x, clamp(t.y + 2 * y, 0, size.y - 1));
            vec4 s = imageLoad(GYROFLOW_SPHERE, p);
            float age = PTS + 1000.0 - s.a;
            if (age >= -0.05 && age < 10.0) {
                float k = exp2(-max(age, 0.0) / 2.5);
                sum += k * s.rgb;
                ws += k;
            }
        }
    }
    if (ws <= 0.0) return base;
    return vec4(mix(base.rgb, sum / ws, clamp(ws / 9.0 * 1.5, 0.0, 1.0)), base.a);
}
#endif

// The fill where the frame doesn't cover the output: the sphere trail over the smooth fill, or the smooth fill
vec4 fill_at(vec2 uv)
{
#if SPHERE_FILL
    return sphere_fill(uv);
#elif EDGE_FILL
    return edge_fill(uv);
#else
    return BACKGROUND;
#endif
}

vec4 finish(vec2 uv)
{
    if (outside(uv)) {
#ifdef UV_OUT
        return BACKGROUND;
#else
        return fill_at(uv);
#endif
    }
#ifdef UV_OUT
    // The tests' output (gpu_table.rs): the source coordinate, not the picture
    return vec4(uv, 0.0, 1.0);
#else
    vec2 t = texel(uv);
#if INTERP == 4
    vec2 ux = dFdx(t), uy = dFdy(t);
    vec4 c = sample_src(t, vec4(ux.x, uy.x, ux.y, uy.y));
#elif INTERP == 2 || INTERP == 3
    vec2 ux = dFdx(t), uy = dFdy(t);
    vec4 c = sample_src(t, clamp(max(abs(ux), abs(uy)), 1.0, 4.0));
#else
    vec4 c = sample_src(t);
#endif
#if EDGE_FILL
    // Near the frame's edge, fading into the fill
    float m = feather(uv);
    if (m < 1.0) c = mix(fill_at(uv), c, m);
#endif
    return c;
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
#if EDGE_FILL
    float m = feather(uv);
    if (m < 1.0) {
        vec4 f = fill_at(uv);
        c.r = mix(f.r, c.r, m);
        c.b = mix(f.b, c.b, m);
    }
#endif
#else
    c.r = finish(rb.xy).r;
    c.b = finish(rb.zw).b;
#endif
    return c;
#endif
}
#endif
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
    cur_j = j;
    vec2 uv;
    want_boundary = EDGE_FILL == 1;
    if (!source_coord(j, uv)) {
#if EDGE_FILL && !defined(UV_OUT)
        // Past the lens's field: the fill too, muted fully where the ray has no place in the frame at all
        return fill_at(boundary_uv ? uv : vec2(-1e4));
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
    cur_j = j;
    vec2 uv;
    // The frame's parameters only where the grid has no image (each is a fetch per pixel). Not one condition with &&:
    // naga evaluates both calls, and the second overwrites uv
    bool ok = grid_coord(j, uv);
#ifndef GRID_STRICT
    // The tests leave this out: the grid alone. Where the ray has no image, the edge fill from where the lens's field ends
    if (!ok) {
        want_boundary = EDGE_FILL == 1;
        ok = frame_setup() && source_coord(j, uv);
#if EDGE_FILL && !defined(UV_OUT)
        // Past the lens's field: the fill too, muted fully where the ray has no place in the frame at all
        if (!ok) return fill_at(boundary_uv ? uv : vec2(-1e4));
#endif
    }
#endif
    if (!ok) return BACKGROUND;
#if CHROMATIC
    if (grid_wet) return finish_rgb(uv, grid_rb);
#endif
    return finish(uv);
}

// ==== pass fill_lo ====
// The smooth fill (edges.rs, smooth.rs) on a coarse grid of the output (FILL_W x FILL_H, ~8 px a cell at 1080p), its
// first step: each cell the frame's average there (3x3 places, the frame's edge repeated past it) and how much of it
// the frame covers, its feather counted (w)
vec4 hook()
{
    vec2 cell = gl_FragCoord.xy - 0.5;
    vec4 sum = vec4(0.0);
    float cover = 0.0;
    int n = 0;
    if (frame_setup()) {
        for (int t = 0; t < 9; t++) {
            vec2 j = (cell + (vec2(t % 3, t / 3) + 0.5) / 3.0) * target_size / vec2(FILL_W, FILL_H) - 0.5;
            vec2 uv;
#ifdef GRID_STEP
            // The warp's own grid where it has the place (the frame's parameters are set either way)
            bool ok = grid_coord(j, uv);
            if (!ok) ok = source_coord(j, uv);
#else
            bool ok = source_coord(j, uv);
#endif
            if (!ok) continue;
            sum += src(texel(clamp(uv, vec2(0.0), vec2(SRC_W, SRC_H) - 1.0)));
            cover += feather(uv);
            n++;
        }
    }
    if (n == 0) return vec4(0.0);
    vec4 c = sum / float(n);
    c.w = cover / 9.0;
    return c;
}

// ==== pass fill_pull ====
// Pull-push (The Lumigraph, 1996; smooth.rs), down: each cell the average of the four below it, weighted by how much
// of each is known, and known as much as they add up to, at most fully
vec4 hook()
{
    ivec2 c = ivec2(gl_FragCoord.xy);
    ivec2 fs = ivec2(IN_SIZE);
    vec3 acc = vec3(0.0);
    float total = 0.0;
    for (int i = 0; i < 4; i++) {
        ivec2 f = 2 * c + ivec2(i % 2, i / 2);
        if (any(greaterThanEqual(f, fs))) continue;
        vec4 v = texelFetch(IN_RAW, f, 0) * IN_MUL;
        if (v.w <= 0.0) continue;
        acc += v.xyz * v.w;
        total += v.w;
    }
    if (total > 1e-6) acc /= total;
    return vec4(acc, min(total, 1.0));
}

// ==== pass fill_push ====
// And up: each cell keeps what it knows, the rest from the coarser one (as filled already), bilinear
vec4 hook()
{
    ivec2 x = ivec2(gl_FragCoord.xy);
    vec4 f = texelFetch(IN_RAW, x, 0) * IN_MUL;
    if (f.w >= 1.0) return vec4(f.xyz, 1.0);
    vec2 cs = COARSE_SIZE;
    vec2 p = clamp((vec2(x) + 0.5) * 0.5, vec2(0.5), cs - 0.5);
    vec4 c = texture(COARSE_RAW, p / cs) * COARSE_MUL;
    return vec4(f.xyz * f.w + c.xyz * (1.0 - f.w), 1.0);
}

// ==== pass fill_blur ====
// Then blurred, a Gaussian of EDGE_BLUR of the height, one way (FILL_DIR) at a time, its ends repeated
vec4 hook()
{
    ivec2 x = ivec2(gl_FragCoord.xy);
    ivec2 sz = ivec2(IN_SIZE);
    float sigma = max(EDGE_BLUR * float(FILL_H), 0.3);
    int r = min(int(ceil(3.0 * sigma)), 24);
    vec4 acc = vec4(0.0);
    float ws = 0.0;
    for (int k = -r; k <= r; k++) {
        float w = exp(-float(k * k) / (2.0 * sigma * sigma));
        acc += w * texelFetch(IN_RAW, clamp(x + k * FILL_DIR, ivec2(0), sz - 1), 0);
        ws += w;
    }
    return acc * IN_MUL / ws;
}

// ==== pass sphere ====
#ifdef SPHERE_PASS
ivec2 sphere_texel(vec3 w)
{
    vec2 size = vec2(imageSize(GYROFLOW_SPHERE));
    float lon = atan(w.x, w.z), lat = asin(clamp(w.y / length(w), -1.0, 1.0));
    return ivec2(floor(vec2(lon / (2.0 * PI) + 0.5, lat / PI + 0.5) * size));
}
#endif

void hook()
{
    // Every other output pixel each way (a texel of the sphere is ~3.5 of a 1080p frame's): where the frame covers it,
    // its color, by its direction of the world, with the time it was seen
#ifdef SPHERE_PASS
    vec2 j = vec2(gl_GlobalInvocationID.xy) * 2.0;
    if (any(greaterThanEqual(j, target_size))) return;
    vec2 uv;
    if (!grid_coord(j, uv) || any(lessThan(uv, vec2(1.0))) || any(greaterThan(uv, vec2(SRC_W, SRC_H) - 2.0))) return;
    if (!frame_setup()) return;
    vec4 c = HOOKED_mul * texelFetch(HOOKED_raw, ivec2(j), 0);
    imageStore(GYROFLOW_SPHERE, sphere_texel(qrot(table(15), output_ray(j))), vec4(c.rgb, PTS + 1000.0));
#endif
}

// ==== pass dn_luma ====
// Temporal denoise (the clean frame: Ctrl+p), after the warp at MAIN, in RGB. The paused frame's way
// (vs-gyroflow/src/still.rs): each pixel is the average of it and its pixels in the DN_RADIUS frames before it and the
// DN_RADIUS after it, where the motion found between them puts them, each weighted by how much the patches around them
// differ beyond the noise. Where something moved in a way the motion doesn't follow, the patches differ and the frame
// keeps its own pixel: no trails. The frames after it are frames played already: the picture shown is the one
// gyroflow_dn_after frames before the newest (main.lua delays the audio as much; 0 in crop mode, whose camera turns the
// newest frame as it is dragged), and after a seek the first frame stays until the ones after it are there.
// The frames, as decoded and warped, in a ring of DN_RING slots in one texture (GDN_FRAMES, DN_COLS slots across) and
// their quarter-size luma in another (GDN_QUART), which keeps the storage images per pass few (MoltenVK's limit).
// The motion is searched once per frame, between it and the one before (per 32 px block: the gyro's prediction, a
// search around it on the quarter-size luma, then at full size; GDN_VEC, a field per slot, kept). The motion from the
// shown frame to each other one is those steps chained (backwards; forwards, each step inverted), then checked within
// a pixel at full size (GDN_NV, a texture the blend reads through the sampler's filter). The noise: by brightness, from
// how the shown frame differs from the one before it where they match (dn_blend, dn_state). This pass: the frame's luma at a quarter of its size, for the search
vec4 hook()
{
    vec2 p = HOOKED_pos;
    vec2 d = HOOKED_pt;
    vec3 c = (HOOKED_tex(p + vec2(-d.x, -d.y)).rgb + HOOKED_tex(p + vec2(d.x, -d.y)).rgb
            + HOOKED_tex(p + vec2(-d.x, d.y)).rgb + HOOKED_tex(p + vec2(d.x, d.y)).rgb) * 0.25;
    return vec4(dot(c, vec3(0.2627, 0.678, 0.0593)));
}

// ==== pass dn_common ====
#ifdef DN_PASS
#define DN_SLOT ivec2(DN_SLOT_W, DN_SLOT_H)
#define DN_MAGIC 0x5E21u
float lum(vec3 c) { return dot(c, vec3(0.2627, 0.678, 0.0593)); }
ivec2 dn_cell(int s) { return ivec2(s % DN_COLS, s / DN_COLS); }
// The ring holds frames of this render's kind: not after a reset (main.lua's epoch: the camera's mode, a realign) or a
// change of the output's size
bool dn_ok() { return gdn_magic == DN_MAGIC && gdn_epoch == gyroflow_dn_epoch && all(equal(gdn_size, target_size)); }
// This render's frame is the newest one again (paused, a redraw)
bool dn_same() { return dn_ok() && abs(PTS - gdn_pts[gdn_head]) < 1e-4; }
// The frames on each side it takes (Performance & Quality's temporal denoise: Full 5, Light 3), and of them the ones
// after the shown frame
int dn_radius() { return clamp(gyroflow_dn_radius, 1, DN_RADIUS); }
int dn_after() { return clamp(gyroflow_dn_after, 0, dn_radius()); }
// This render writes its frame: a new one, or the same one again where none is shown after it (crop mode's camera
// turns it as it's dragged)
bool dn_fresh() { return !dn_same() || dn_after() == 0; }
int dn_newest() { return !dn_ok() ? 0 : dn_same() ? gdn_head : (gdn_head + 1) % DN_RING; }

// The window (dn_window sets it, once a pass): the newest frame's slot, the shown frame (dn_kc before the newest), how
// many it has before it and after it. Only frames in a row (none missing past a dropped one, none from before a seek)
int dn_nw, dn_kc, dn_nb_back, dn_nb_fwd;
// The slot of the frame k before the newest
int dn_back_slot(int k) { return (dn_nw - k + 2 * DN_RING) % DN_RING; }
void dn_window()
{
    bool ok = dn_ok();
    dn_nw = dn_newest();
    float pts[2 * DN_RADIUS + 1];
    for (int k = 0; k <= 2 * DN_RADIUS; k++) {
        int s = dn_back_slot(k);
        pts[k] = k == 0 ? PTS : ok ? gdn_pts[s] : -1e9;
    }
    int back = 0;
    for (int k = 0; k < 2 * DN_RADIUS; k++) {
        float dt = pts[k] - pts[k + 1];
        if (!(dt > 1e-4 && dt < 2.6 / FPS)) break;
        back++;
    }
    dn_kc = min(dn_after(), back);
    dn_nb_fwd = dn_kc;
    dn_nb_back = min(dn_radius(), back - dn_kc);
}
int dn_center() { return dn_back_slot(dn_kc); }
// The slot o frames after the shown one (o < 0: before it)
int dn_slot_at(int o) { return dn_back_slot(dn_kc - o); }
// Neighbor n of the shown frame: 0 .. DN_RADIUS - 1 the frames before it, nearest first, then those after it
int dn_offset(int n) { return n < DN_RADIUS ? -(n + 1) : n - DN_RADIUS + 1; }
bool dn_has(int n) { int o = dn_offset(n); return o < 0 ? -o <= dn_nb_back : o <= dn_nb_fwd; }

vec3 dn_frame(int s, ivec2 p) { p = clamp(p, ivec2(0), ivec2(target_size) - 1); return imageLoad(GDN_FRAMES, p + dn_cell(s) * DN_SLOT).rgb; }
float dn_quart(int s, ivec2 p) { p = clamp(p, ivec2(0), ivec2(target_size / 4.0) - 1); return imageLoad(GDN_QUART, p + dn_cell(s) * (DN_SLOT / 4)).x; }
ivec2 dn_blocks() { return ivec2(ceil(target_size / 32.0)); }
#endif

// ==== pass dn_store ====
#ifdef DN_STORE
void hook()
{
    // The frame into its slot, as decoded and warped; its quarter-size luma too
    ivec2 j = ivec2(gl_GlobalInvocationID.xy);
    if (any(greaterThanEqual(j, ivec2(target_size))) || any(greaterThan(ivec2(target_size), DN_SLOT)) || !dn_fresh()) return;
    int s = dn_newest();
    imageStore(GDN_FRAMES, j + dn_cell(s) * DN_SLOT, vec4((HOOKED_mul * texelFetch(HOOKED_raw, j, 0)).rgb, 1.0));
    if (all(equal(j % 4, ivec2(0))))
        imageStore(GDN_QUART, j / 4 + dn_cell(s) * (DN_SLOT / 4), vec4(texelFetch(GDN_CUR4_raw, j / 4, 0).x * GDN_CUR4_mul));
}
#endif

// ==== pass dn_motion ====
#ifdef DN_MOTION
float cur4(ivec2 p) { return texelFetch(GDN_CUR4_raw, clamp(p, ivec2(0), ivec2(GDN_CUR4_size) - 1), 0).x * GDN_CUR4_mul; }
float cur1(ivec2 p) { return lum(HOOKED_mul * texelFetch(HOOKED_raw, clamp(p, ivec2(0), ivec2(HOOKED_size) - 1), 0).rgb); }

// Where output pixel j was in the frame before (its camera q; crop mode's camera cam and zoom as it was rendered) by
// the gyro alone: its ray turned into the world by this frame's camera, out of it by that one's
vec2 predict(vec2 j, vec4 qp, vec4 cam, float zoom)
{
    vec3 w = qrot(table(15), output_ray(j));
    vec3 d = qrot(vec4(-cam.xyz, cam.w), qrot(vec4(-qp.xyz, qp.w), w));
    if (d.z < 1e-3) return j;
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * zoom);
    float rxy = max(length(d.xy), 1e-9);
#if PROJ == 0
    vec2 n = d.xy / d.z;
#else
    vec2 n = d.xy / rxy * atan(rxy, d.z);
#endif
    return (n * out_f + 0.5 * size - t2.xy) * target_size / size;
}

void hook()
{
    // The newest frame's blocks: where each is in the frame before it
    ivec2 nb = dn_blocks();
    ivec2 b = ivec2(gl_GlobalInvocationID.xy);
    if (any(greaterThanEqual(b, nb)) || !dn_fresh()) return;
    dn_window();
    int s = dn_nw;
    vec4 res = vec4(0.0, 0.0, 1e9, 0.0);
    if (dn_kc + dn_nb_back >= 1 && frame_setup()) {
        int slot = dn_back_slot(1);
        vec2 c = (vec2(b) + 0.5) * 32.0;
        vec2 pred = predict(c, gdn_quat[slot], gdn_cam[slot], gdn_zoom[slot]) - c;
        // Quarter size: 8x8 of the block, within 2 of the prediction, the block and its window read once; a candidate
        // has to beat the prediction by a fifth of its cost (flat noisy water would follow the noise, or the light's fall)
        ivec2 o = b * 8;
        ivec2 pq = ivec2(round(pred / 4.0));
        float cb[64];
        for (int t = 0; t < 64; t++) cb[t] = cur4(o + ivec2(t % 8, t / 8));
        float win[144];
        for (int t = 0; t < 144; t++) win[t] = dn_quart(slot, o + pq + ivec2(t % 12 - 2, t / 12 - 2));
        float best = 1e9;
        ivec2 bv = pq;
        for (int dy = -2; dy <= 2; dy++) {
            for (int dx = -2; dx <= 2; dx++) {
                float sad = 0.0;
                for (int t = 0; t < 64; t++) sad += abs(cb[t] - win[(t / 8 + dy + 2) * 12 + t % 8 + dx + 2]);
                if (dx == 0 && dy == 0) sad *= 0.8;
                if (sad < best) { best = sad; bv = pq + ivec2(dx, dy); }
            }
        }
        // Full size: every fourth pixel of the 32x32 block
        ivec2 o1 = b * 32;
        ivec2 base = bv * 4;
        float c1[64];
        for (int t = 0; t < 64; t++) c1[t] = cur1(o1 + 4 * ivec2(t % 8, t / 8));
        // Within a pixel (3x3), and between whole pixels where the best has costs on both sides each way: the steps
        // are chained, so their halves of a pixel would add up
        float cost[9];
        float best1 = 1e9;
        int bi = 4;
        for (int dy = -1; dy <= 1; dy++) {
            for (int dx = -1; dx <= 1; dx++) {
                ivec2 cand = base + ivec2(dx, dy);
                float sad = 0.0;
                for (int t = 0; t < 64; t++) sad += abs(c1[t] - lum(dn_frame(slot, o1 + 4 * ivec2(t % 8, t / 8) + cand)));
                if (dx == 0 && dy == 0) sad *= 0.9;
                int i = (dy + 1) * 3 + dx + 1;
                cost[i] = sad;
                if (sad < best1) { best1 = sad; bi = i; }
            }
        }
        ivec2 bc = ivec2(bi % 3, bi / 3);
        vec2 sub = vec2(0.0);
        if (bc.x == 1) {
            float l = cost[bi - 1], r = cost[bi + 1], den = l - 2.0 * best1 + r;
            if (den > 1e-6) sub.x = clamp(0.5 * (l - r) / den, -0.5, 0.5);
        }
        if (bc.y == 1) {
            float u = cost[bi - 3], d = cost[bi + 3], den = u - 2.0 * best1 + d;
            if (den > 1e-6) sub.y = clamp(0.5 * (u - d) / den, -0.5, 0.5);
        }
        res = vec4(vec2(base + bc - 1) + sub, best1 / 64.0, 1.0);
    }
    imageStore(GDN_VEC, ivec2(b.x, b.y + s * nb.y), res);
}
#endif

// ==== pass dn_compose ====
#ifdef DN_COMPOSE
// Slot s's motion (its blocks: where they are in the frame before it) at its pixel p, between the four blocks around
// it. False where one of them has none
bool dn_step(int s, vec2 p, out vec2 v)
{
    ivec2 nb = dn_blocks();
    vec2 bp = p / 32.0 - 0.5;
    ivec2 b0 = ivec2(floor(bp));
    vec2 f = clamp(bp - vec2(b0), 0.0, 1.0);
    ivec2 c0 = clamp(b0, ivec2(0), nb - 1), c1 = clamp(b0 + 1, ivec2(0), nb - 1);
    int r = s * nb.y;
    vec4 v00 = imageLoad(GDN_VEC, ivec2(c0.x, c0.y + r)), v10 = imageLoad(GDN_VEC, ivec2(c1.x, c0.y + r));
    vec4 v01 = imageLoad(GDN_VEC, ivec2(c0.x, c1.y + r)), v11 = imageLoad(GDN_VEC, ivec2(c1.x, c1.y + r));
    v = mix(mix(v00.xy, v10.xy, f.x), mix(v01.xy, v11.xy, f.x), f.y);
    return v00.w > 0.5 && v10.w > 0.5 && v01.w > 0.5 && v11.w > 0.5;
}

vec4 hook()
{
    // x: the block across, y: the block down and the neighbor (the blocks of neighbor n below those of n - 1, a band
    // of a row more than the blocks down each)
    ivec2 nb = dn_blocks();
    ivec2 g = ivec2(gl_FragCoord.xy);
    int band = nb.y + 1;
    int n = g.y / band;
    ivec2 b = ivec2(g.x, g.y % band);
    if (any(greaterThanEqual(b, nb)) || n >= 2 * DN_RADIUS) return vec4(0.0);
    dn_window();
    if (!dn_has(n)) return vec4(0.0);
    int o = dn_offset(n);
    vec2 c = (vec2(b) + 0.5) * 32.0;
    vec2 p = c;
    if (o < 0) {
        // Back: each frame's own step to the one before it
        for (int i = 0; i < -o; i++) {
            vec2 v;
            if (!dn_step(dn_slot_at(-i), p, v)) return vec4(0.0);
            p += v;
        }
    } else {
        // Ahead: the step of the frame after, inverted (the point q of it that its step brings to p)
        for (int i = 1; i <= o; i++) {
            int s = dn_slot_at(i);
            vec2 q = p;
            for (int it = 0; it < 3; it++) {
                vec2 v;
                if (!dn_step(s, q, v)) return vec4(0.0);
                q = p - v;
            }
            p = q;
        }
    }
    // The frame just before: its own step, searched and fitted between pixels already (dn_motion), as it is
    if (o == -1) return vec4(p - c, 0.0, 1.0);
    // Within a pixel of the chain's answer at full size (3x3), 16 pixels of the 32x32 block (every eighth across and
    // down, the rows shifted by four); the answer itself kept unless another place is clearly better
    int sc = dn_center(), sn = dn_slot_at(o);
    ivec2 o1 = b * 32;
    vec2 v0 = p - c;
    float c1[16];
    for (int t = 0; t < 16; t++) c1[t] = lum(dn_frame(sc, o1 + ivec2(8 * (t % 4) + 4 * ((t / 4) % 2) + 2, 8 * (t / 4) + 2)));
    float best = 1e9;
    vec2 bv = v0;
    for (int k = 0; k < 9; k++) {
        // The answer first, then the eight around it
        ivec2 off = k == 0 ? ivec2(0) : ivec2((k - 1) % 3 - 1, (k - 1) / 3 - 1);
        if (k > 0 && off == ivec2(0)) off = ivec2(1, 1);
        ivec2 cand = ivec2(round(v0)) + off;
        float sad = 0.0;
        for (int t = 0; t < 16; t++) sad += abs(c1[t] - lum(dn_frame(sn, o1 + ivec2(8 * (t % 4) + 4 * ((t / 4) % 2) + 2, 8 * (t / 4) + 2) + cand)));
        if (k == 0) sad *= 0.85;
        if (sad < best) { best = sad; bv = k == 0 ? v0 : vec2(cand); }
    }
    return vec4(bv, best / 16.0, 1.0);
}
#endif

// ==== pass dn_blend ====
#ifdef DN_BLEND
// How much a patch may differ beyond the noise and still be averaged in: still.rs's weight's scale
#define DN_TOLERANCE 1.5
// The noise at a brightness, between its bins: the luma's (gdn_sigma[0-15]) and the color's (B-Y, gdn_sigma[16-31]);
// a bin not measured yet holds the start value negated (dn_state)
float sigma_at(float l)
{
    float b = clamp(l * 16.0 - 0.5, 0.0, 15.0);
    int i = int(b);
    return mix(abs(gdn_sigma[i]), abs(gdn_sigma[min(i + 1, 15)]), b - float(i));
}
float sigc_at(float l)
{
    float b = clamp(l * 16.0 - 0.5, 0.0, 15.0);
    int i = int(b);
    return mix(abs(gdn_sigma[16 + i]), abs(gdn_sigma[16 + min(i + 1, 15)]), b - float(i));
}
vec2 chroma(vec3 c) { float y = lum(c); return vec2(c.b - y, c.r - y); }
vec3 frame_bilinear(int slot, vec2 p)
{
    vec2 f = fract(p);
    ivec2 i = ivec2(floor(p));
    return mix(mix(dn_frame(slot, i), dn_frame(slot, i + ivec2(1, 0)), f.x), mix(dn_frame(slot, i + ivec2(0, 1)), dn_frame(slot, i + ivec2(1, 1)), f.x), f.y);
}

vec4 hook()
{
    ivec2 j = ivec2(HOOKED_pos * HOOKED_size);
    if (any(greaterThan(ivec2(target_size), DN_SLOT))) return HOOKED_tex(HOOKED_pos);
    dn_window();
    int sc = dn_center();
    vec3 cur = dn_frame(sc, j);
    ivec2 nb = dn_blocks();
    int band = nb.y + 1;
    // Where the pixel is among the blocks' centers (texels of GDN_NV), kept within the blocks
    vec2 bt = clamp((vec2(j) + 0.5) / 32.0, vec2(0.5), vec2(nb) - 0.5);
    // The shown frame's own patch: the pixel, and the 4x4 pixels around it (the quarter-size luma: a match of what's
    // around is told from the noise better, at one read; its noise ~1/1.8 of a pixel's, the warp's interpolation
    // having tied neighbors together, as measured on clip 0126)
    float l = lum(cur);
    float own_q = dn_quart(sc, j / 4);
    float s = max(sigma_at(l), 1e-4);
    // The reference the other frames are matched against: the pixel kept within the noise of its four neighbours'
    // mean, so a spike of the noise doesn't turn every other frame away (and stay, a speck on a clean background)
    vec3 n4 = dn_frame(sc, j + ivec2(1, 0)) + dn_frame(sc, j - ivec2(1, 0)) + dn_frame(sc, j + ivec2(0, 1)) + dn_frame(sc, j - ivec2(0, 1));
    float m4 = 0.25 * lum(n4);
    float lr = m4 + clamp(l - m4, -s, s);
    // The color's: the pixel's and its four neighbours' (the color's noise is coarser than the luma's)
    vec2 cref = chroma((cur + n4) * 0.2);
    float sc2 = max(sigc_at(l), 1e-4);
    vec3 sum = cur;
    float wsum = 1.0;
    float noise_diff = -1.0, noise_c = -1.0;
    for (int n = 0; n < 2 * DN_RADIUS; n++) {
        if (!dn_has(n)) continue;
        int slot = dn_slot_at(dn_offset(n));
        // The pixel's vector, between the four blocks around it (the sampler's filter), none where one has none
        vec4 vv = GDN_NV_tex((bt + vec2(0.0, float(n * band))) / GDN_NV_size);
        if (vv.w < 0.999) continue;
        vec2 p = vec2(j) + vv.xy;
        // The four pixels around where it is: its color between them, and the nearest one for the patch
        vec2 f = fract(p);
        ivec2 i = ivec2(floor(p));
        vec3 a00 = dn_frame(slot, i), a10 = dn_frame(slot, i + ivec2(1, 0)), a01 = dn_frame(slot, i + ivec2(0, 1)), a11 = dn_frame(slot, i + ivec2(1, 1));
        vec3 h = mix(mix(a00, a10, f.x), mix(a01, a11, f.x), f.y);
        vec3 near = f.y < 0.5 ? (f.x < 0.5 ? a00 : a10) : (f.x < 0.5 ? a01 : a11);
        float e = lr - lum(near);
        float eq = 1.8 * (own_q - dn_quart(slot, ivec2(floor((p + 0.5) / 4.0))));
        float d = 0.5 * (e * e + eq * eq);
        // still.rs's weight: full where the patches differ by the noise alone, none past three times it
        float w = 1.0 - smoothstep(1.0 * DN_TOLERANCE, 3.0 * DN_TOLERANCE, d / (2.0 * s * s));
        // And the color: a fin of another color but the same brightness isn't the water behind it
        vec2 dc = cref - chroma(near);
        w *= 1.0 - smoothstep(3.0, 8.0, dot(dc, dc) / (2.4 * sc2 * sc2));
        sum += w * h;
        wsum += w;
        // The noise's sample: against the frame before, its nearest pixel (between pixels, the bilinear mix would
        // halve it)
        if (n == 0) {
            noise_diff = abs(l - lum(near));
            noise_c = abs(chroma(cur).x - chroma(near).x);
        }
    }
    vec3 den = sum / wsum;
    // The noise, from the shown frame against the one before where they match
    if (!dn_same() && noise_diff >= 0.0 && all(equal(j % 4, ivec2(0)))) {
        uint bin = uint(clamp(l * 16.0, 0.0, 15.0));
        uint st = uint(clamp(noise_diff * 4096.0, 0.0, 255.0));
        atomicAdd(gdn_hist[bin * 256u + st], 1u);
        atomicAdd(gdn_hist[4096u + bin * 256u + uint(clamp(noise_c * 4096.0, 0.0, 255.0))], 1u);
    }
    // Some of the grain kept, so it doesn't look smeared (gyroflow.conf's still_grain)
    return vec4(den + gyroflow_dn_keep * (cur - den), 1.0);
}
#endif

// ==== pass dn_state ====
#ifdef DN_STATE
void hook()
{
    // One workgroup of 16: brightness bin i each, the luma's and the color's
    uint i = gl_LocalInvocationID.x;
    bool same = dn_same(), ok = dn_ok(), fresh = dn_fresh();
    int nw = dn_newest();
    bool first = gdn_magic != DN_MAGIC;
    memoryBarrierBuffer();
    barrier();
    for (uint k = i; k < 32u && !same; k += 16u) {
        // The noise of bin k: the upper quartile of the differences over 1.6268 (two noisy samples' |a - b| is under
        // it for three quarters of them). Not the lower quartile (still.rs's): HEVC copies many blocks from the frame
        // before, whose difference is about none, and the rest is fresh noise, so the lower quartile read the noise
        // two or three times too low, swinging with each frame (clip 0087: 2-10 codes for about 8), and the noise was
        // taken for motion. Smoothed over the frames (a tenth of each); a bin with too few keeps what it had, and
        // starts from 4 codes, negated until it's measured
        uint total = 0u;
        for (uint st = 0u; st < 256u; st++) total += gdn_hist[k * 256u + st];
        if (total >= 200u) {
            uint seen = 0u;
            float q = 0.0;
            for (uint st = 0u; st < 256u; st++) {
                uint h = gdn_hist[k * 256u + st];
                if (float(seen + h) >= 0.75 * float(total)) { q = (float(st) + 0.5) / 4096.0; break; }
                seen += h;
            }
            float v = max(q / 1.6268, 0.0005);
            gdn_sigma[k] = first || gdn_sigma[k] < 0.0 ? v : mix(gdn_sigma[k], v, 0.1);
        } else if (first) {
            gdn_sigma[k] = -0.004;
        }
        for (uint st = 0u; st < 256u; st++) gdn_hist[k * 256u + st] = 0u;
    }
    memoryBarrierBuffer();
    barrier();
    if (i == 0u && fresh) {
        // The frame in its slot now (dn_store put it there): its time and cameras, for the frames after it
        if (!ok)
            for (int k = 0; k < DN_RING; k++) gdn_pts[k] = -1e9;
        gdn_head = nw;
        gdn_pts[nw] = PTS;
        if (frame_setup()) gdn_quat[nw] = table(15);
        gdn_cam[nw] = vec4(gyroflow_cam_x, gyroflow_cam_y, gyroflow_cam_z, gyroflow_cam_w);
        gdn_zoom[nw] = gyroflow_cam_zoom;
        gdn_size = target_size;
        gdn_epoch = gyroflow_dn_epoch;
        gdn_magic = DN_MAGIC;
    }
}
#endif

// ==== pass sphere360 ====
// A 360 camera's sphere (the GPU path's 360 mode: gpu_table::sphere_shader; docs/gpu-render.md, "360 on the GPU"):
// the lens tracks joined side by side (mpv's lavfi-complex hstack) in, the equirect sphere in the camera's axes out,
// as vs-gyroflow's Sphere360 renders it: each lens through gyroflow-core's kernel (the equirect output, its RigLens
// folded into its rows' rotations and translation3d, its validity the field limit, its rolling shutter), blended by
// its share of each direction (rig.rs's weights, a texture a tenth of the sphere's size). Under it the stamp's two
// rows, its codes as gray (view360.glsl reads them at MAIN, gyroflow_stamp360; a shader's saved textures are its own,
// so the stamp goes in the picture as Sphere360's does). target_size is the sphere's, without them (gpu_table defines
// it so, ahead of the common code)

vec4 hook()
{
    vec2 px = HOOKED_pos * vec2(OUT_W, OUT_H + 2);
    if (px.y > float(OUT_H)) {
        // The stamp: code i a 2x2 block of gray from the left (stamp.rs's layout), its value from the table
        int i = int(px.x) / 2;
        if (i >= 34) return vec4(0.0, 0.0, 0.0, 1.0);
        lens_base = 0;
        frame_setup();
        lens_base = STAMP_BASE;
        float v = table(i / 4)[i % 4];
        return vec4(v, v, v, 1.0);
    }
    vec2 j = px - 0.5;
    vec4 w = texture(GYROFLOW_WEIGHTS, px / vec2(OUT_W, OUT_H));
    vec4 sum = vec4(0.0);
    float total = 0.0;
    for (int l = 0; l < LENSES; l++) {
        float wl = w[l];
        if (wl <= 0.0) continue;
        lens_base = l * LENS_BLOCK;
        if (!frame_setup()) continue;
        vec2 uv;
        if (!source_coord(j, uv)) continue;
        if (any(lessThan(uv, vec2(-0.5))) || any(greaterThan(uv, vec2(SRC_W, SRC_H) - 0.5))) continue;
        // The lens's part of the joined frame, bilinear
        sum += wl * HOOKED_tex((uv + 0.5 + LENS_OFFSET[l]) / HOOKED_size);
        total += wl;
    }
    return total > 0.0 ? vec4(sum.rgb / total, 1.0) : vec4(0.0, 0.0, 0.0, 1.0);
}
