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
// No comments between the directive blocks either: libplacebo reads them as the block before's data (a PARAM's value).
//
// Where a moment starts in its clip (gyroflow_time_offset, s) is a parameter, not a define: each moment's shader then
// has the same code as the others of its kind, and mpv takes its pipelines from its cache instead of building them
// again (~2.5 s on an M-series Mac). gyroflow-gpu-table writes its value; nothing changes it. The clip's frame rate
// (gyroflow_fps) likewise: a clip of another frame rate shares the pipelines (tools/pipebench: a new rate cost 37 passes,
// 3-8 s the first time).

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

//!PARAM gyroflow_dn_show
//!TYPE DYNAMIC float
-1.0

//!PARAM gyroflow_render_scale
//!TYPE DYNAMIC float
//!MINIMUM 0.1
//!MAXIMUM 4.0
1.0

//!PARAM gyroflow_dn_radius
//!TYPE DYNAMIC int
//!MINIMUM 1
//!MAXIMUM 5
5

//!PARAM gyroflow_dn_strong
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 1
0

//!PARAM gyroflow_dn_keep
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 1.0
0.25

//!PARAM gyroflow_trans
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 1.0
1.0

//!PARAM gyroflow_time_offset
//!TYPE DYNAMIC float
@TIME_OFFSET@

//!PARAM gyroflow_fps
//!TYPE DYNAMIC float
@FPS@

//!TEXTURE GYROFLOW_TABLE
//!SIZE @TABLE_W@ @TABLE_H@
//!FORMAT rgba32f
//!FILTER NEAREST
@TABLE_HEX@

@PASSES@

// ==== common ====
// The size this renders at (RT): the video's rectangle on screen (target_size) times gyroflow_render_scale, which
// main.lua sets from the resolution chosen (Performance & Quality: at most 1080p or 720p, the video's own size), as
// libplacebo rounds the passes' sizes; mpv scales the result to the screen. A shader of its own size defines it first
#ifndef RT
#define RT round(target_size * gyroflow_render_scale)
#endif
#ifndef LENS_BLEND
#define LENS_BLEND 0
#endif

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
// ray at the edge of what the lens sees lands (boundary_uv set), for the edge fill to take its colors from, and how far
// past that edge the ray is (boundary_excess, radians, near enough): every ray past the field lands on the same circle,
// so the distance from the frame stops growing there, and anything that eases out with it (the sphere's continuation)
// would show the circle
bool want_boundary = false;
bool boundary_uv = false;
float boundary_excess = 0.0;
bool rotate_and_distort(vec3 d, vec4 q, out vec2 uv)
{
    vec3 p = qrot(q, d) + t3;
    float rxy = length(p.xy);
    float theta = atan(rxy, p.z);
    uv = vec2(0.0);
    boundary_uv = false;
    boundary_excess = 0.0;
    bool fail = false;
    if (lens.w > 0.0 && theta > lens.w) {
        if (!want_boundary || rxy < 1e-12) return false;
        fail = true;
        boundary_excess = theta - lens.w;
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
            boundary_excess += p.z <= 0.0 ? 1.0 : sin_d - 1.0;
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

#ifdef TRANS_BIND
// The translation compensation (vs-gyroflow's translation.rs, computed for the clip ahead and kept: gyroflow-export
// --translation): each frame's shift in GYROFLOW_TRANS, over the output camera's focal length (so it holds in any
// view and zoom), TRANS_PER_ROW frames a row. gyroflow_trans is how much of it (main.lua's Ctrl+j: 0 or 1; no comment
// can go between the parameters at the top, libplacebo reads it as the value). At most 6 % of the output's width
vec2 trans_shift(int f, vec2 out_f)
{
    if (gyroflow_trans <= 0.0) return vec2(0.0);
    vec2 s = texelFetch(GYROFLOW_TRANS, ivec2(f % TRANS_PER_ROW, f / TRANS_PER_ROW), 0).xy * out_f * gyroflow_trans;
    float lim = 0.06 * float(OUT_W), l = length(s);
    return l > lim ? s * (lim / l) : s;
}
#endif

// The frame's parameters, from the table by PTS. False where the table has none
bool frame_setup()
{
    // A moment played as a video of its own starts TIME_OFFSET (gyroflow_time_offset) seconds into the clip
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
#ifdef TRANS_BIND
    // The translation compensation's shift: where the output looks in Gyroflow's frame (output_ray's focal length).
    // A shift, not a turn: the hand's translation moves a subject's plane evenly across the frame, which a shift undoes;
    // a turn that puts the middle in the same place moves the edges further (1 / cos^2 of their angle). Side by side
    // on the clips they were measured on, Anton saw no difference, and the shift is the cheaper
    t2.xy += trans_shift(table_frame, fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * gyroflow_cam_zoom));
#endif
    params.width = float(SRC_W); params.height = float(SRC_H);
    params.output_width = float(OUT_W); params.output_height = float(OUT_H);
    params.flags = HAS_DIGITAL * 2;
    return t2.w >= 0.5;
}

// Output pixel j (0 .. RT - 1, pixel centers at whole numbers) -> its source coordinate in Gyroflow's frame
// (SRC_W x SRC_H, centers at whole numbers): Stabilization::undistort_coord. False where the ray has no image. The
// output pixel goes into the kernel's output frame (OUT_W x OUT_H) as the VapourSynth path maps its pixel j there:
// j * OUT_W / width
// Output pixel j's ray, in the output camera. Crop mode's camera (main.lua's gyroflow_cam_ parameters, set as it turns:
// no new table) turns it first, and zooms by the FOV: gyroflow-core's view_rotation multiplies each row's rotation on
// the right by the view's quaternion in its matrices' axes (gpu_table::camera_quat), the world's too, and its zoom is
// the FOV's (Camera::apply)
// False where output_ray's pixel has no ray (a lens correction below 100 %: past the lens's own image)
bool ray_ok = true;

// The output projection's radius on the plane for a ray of angle theta, and back (projection::project's)
float proj_radius(float theta)
{
#if PROJ == 1
    return 2.0 * tan(theta * 0.5);
#elif PROJ == 2
    return theta;
#elif PROJ == 3
    return 2.0 * sin(theta * 0.5);
#elif PROJ == 4
    return sin(theta);
#else
    return theta < 1.5707 ? tan(theta) : 1e9;
#endif
}

float proj_theta(float r)
{
#if PROJ == 1
    return 2.0 * atan(r * 0.5);
#elif PROJ == 2
    return r;
#elif PROJ == 3
    return 2.0 * asin(min(r * 0.5, 1.0));
#elif PROJ == 4
    return asin(min(r, 1.0));
#else
    return atan(r);
#endif
}

#if LENS_BLEND
// A lens correction below 100 % (Gyroflow's output_ray in the fork, its wgpu_undistort.wgsl): a ray of angle theta lands
// at (1 - a) R(theta) + a P(theta) of the output plane, where the source lens images it mixed with where the output
// projection wants it. Inverted per pixel by regula falsi (Illinois), bracketed by the two maps' own answers.
// Green's index under water (Gyroflow's own coefficient): the blend is where the picture goes, the same for each
// color; each color's own index is applied after, in rotate_and_distort
bool refracts() { return lens.z != 1.0 && lens.z > 0.0; }
float to_water(float th) { return refracts() ? asin(clamp(sin(min(th, 1.5707963)) / lens.z, -1.0, 1.0)) : th; }

float blend_residual(float theta, vec2 u, float t, float a)
{
    float th = theta;
    if (refracts()) th = asin(clamp(sin(th) * lens.z, -1.0, 1.0));
    float s = sin(th), c = cos(th);
    vec2 p = distort_point(u.x * s, u.y * s, c);
    return (1.0 - a) * length(p) + a * proj_radius(theta) - t;
}

// The ray of output point out_pos (out_c the middle, out_f the focal length, fov the zoom), for correction a < 1
bool blend_ray(vec2 out_pos, vec2 out_c, vec2 out_f, float fov, float a, out vec2 ray)
{
    vec2 n_raw = (out_pos - out_c) / out_f;
    ray = unproject(n_raw);
    vec2 n = n_raw;
#if HAS_DIGITAL
    // The digital warp in the un-zoomed (fov 1) frame, so it doesn't depend on the FOV
    vec2 dp = digital_undistort_point((out_pos - out_c) * fov + out_c);
    n = ((dp - out_c) / fov) / out_f;
#endif
    vec2 nb = n * (1.0 - a) + n_raw * a;
    float t = length(nb);
    if (t <= 1e-9) return true;
    vec2 u = nb / t;
    float lo = length(ray);
    vec2 src = undistort_point(n);
    bool has_src = src.x > -99998.0;
    if (!has_src && !(lens.w > 0.0)) return false;
    float hi = to_water(has_src ? length(src) : lens.w);
    bool edge = !has_src;
    if (a > 0.0) hi = min(hi, proj_theta(t / a));
    if (a <= 0.0) {
        if (!has_src) return false;
        // Exactly the source lens's own ray, with the refraction rotate_and_distort applies undone
        ray = src;
        float th = length(src);
        if (refracts() && th > 1e-12) ray = src * (to_water(th) / th);
        return true;
    }
    float l = min(lo, hi), h = max(lo, hi);
    float cap = to_water(lens.w > 0.0 ? lens.w : PI);
    if (h > cap) { h = cap; l = min(l, cap); edge = true; }
    float gl = blend_residual(l, u, t, a), gh = blend_residual(h, u, t, a);
    float theta = h;
    if (gh < 0.0) {
        if (edge) return false;
    } else {
        if (gl >= 0.0) { l = 0.0; gl = -t; }
        int side = 0;
        for (int i = 0; i < 6; i++) {
            float d = gh - gl;
            theta = abs(d) > 1e-12 ? clamp(l - gl * (h - l) / d, l, h) : 0.5 * (l + h);
            float gt = blend_residual(theta, u, t, a);
            if (gt < 0.0) {
                l = theta; gl = gt;
                if (side == -1) gh *= 0.5;
                side = -1;
            } else {
                h = theta; gh = gt;
                if (side == 1) gl *= 0.5;
                side = 1;
            }
        }
    }
    // The source lens's tangential offset belongs to its leg and fades out with it
    vec2 dir = u;
    if (has_src) {
        float sl = length(src), nl = length(n);
        if (sl > 1e-12 && nl > 1e-12) {
            vec2 dd = u + (1.0 - a) * (src / sl - n / nl);
            float dl = length(dd);
            if (dl > 1e-12) dir = dd / dl;
        }
    }
    ray = dir * theta;
    return true;
}
#endif

vec3 output_ray(vec2 j)
{
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_pos = j * size / RT + t2.xy;
    ray_ok = true;
#if PROJ == 5
    // The equirect (a 360 camera's sphere): its plane is the sphere's, all of it 360 x 180 degrees at zoom (fov) 1;
    // longitude across, latitude down, +z at the middle, +x at +90 degrees, up (-y) at the top (the fork's
    // equirect_to_dir)
    vec2 n = (out_pos - 0.5 * size) / (vec2(float(OUT_W) / 6.2831853, float(OUT_H) / 3.1415927) / max(lens.x, 1e-6));
    float cl = cos(n.y);
    vec3 d = vec3(cl * sin(n.x), sin(n.y), cl * cos(n.x));
#else
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * gyroflow_cam_zoom);
#if LENS_BLEND
    vec2 ray;
    if (lens.y < 0.999) ray_ok = blend_ray(out_pos, 0.5 * size, out_f, lens.x * gyroflow_cam_zoom, lens.y, ray);
    else ray = unproject((out_pos - 0.5 * size) / out_f);
    vec3 d = ray_to_dir(ray);
#else
    vec3 d = ray_to_dir(unproject((out_pos - 0.5 * size) / out_f));
#endif
#endif
    return qrot(vec4(gyroflow_cam_x, gyroflow_cam_y, gyroflow_cam_z, gyroflow_cam_w), d);
}

bool source_coord(vec2 j, out vec2 uv)
{
    vec3 d = output_ray(j);
    if (!ray_ok) { uv = vec2(0.0); boundary_uv = false; return false; }
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

#if !defined(GRID_PASS) && !defined(DN_COMPUTE)
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
    // No implicit level of detail: the sphere pass (compute) continues the edge with it too; the grid has no mips
    vec4 f = textureLod(FILL_RAW, (cur_j + 0.5) / RT, 0.0) * FILL_MUL;
    vec2 edge = clamp(uv, vec2(-0.5), vec2(SRC_W, SRC_H) - 0.5);
    float t = clamp(length(uv - edge) / (FILL_MUTE_RAMP * float(SRC_H)), 0.0, 1.0);
    float m = t * t * (3.0 - 2.0 * t);
#if SPHERE_FILL
    // Under the sphere the continuation is written into it and stays: muted, it would leave gray patches no part of
    // the picture has (2026-11-18); the blur alone keeps it from drawing the eye
    float sat = 1.0, bright = 1.0;
#else
    float sat = 1.0 - 0.35 * m, bright = 1.0 - 0.12 * m;
#endif
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
// The sphere (the plugin's sphere trail): an equirect of the world (GYROFLOW_SPHERE, 512 x 256, 0.7 degrees a texel),
// each texel a color and a time: what was seen in that direction (written when seen, pass sphere), or the frame's edge
// continued there (pass sphere where it held nothing, and passes sphere and sphere_beyond where the world changed at
// that part of the frame's edge: pass sphere_edges and sphere_sectors). Read back past the frame (sphere_fill)
//
// Direction w of the world -> the sphere's texel coordinates (texel i covers i .. i + 1)
vec2 sphere_coord(vec3 w)
{
    vec2 size = vec2(imageSize(GYROFLOW_SPHERE));
    float lon = atan(w.x, w.z), lat = asin(clamp(w.y / length(w), -1.0, 1.0));
    return vec2(lon / (2.0 * PI) + 0.5, lat / PI + 0.5) * size;
}

// The texel at texel coordinates tc
ivec2 sphere_texel(vec2 tc, ivec2 size) { return ivec2(int(tc.x) % size.x, clamp(int(tc.y), 0, size.y - 1)); }

// The world's direction of output pixel j (frame_setup first)
vec3 sphere_world(vec2 j) { return qrot(table(15), output_ray(j)); }

// Whether a texel written at time t (PTS + 1000) holds anything: written at all, and not after now (after a seek back).
// A trace doesn't fade with time: it stays until the frame or the edge's continuation is written over it (pass sphere;
// a seek starts the sphere over there)
bool sphere_written(float t) { return t >= 999.0 && t <= PTS + 1000.05; }

#ifndef SPHERE_PASS
// What the sphere holds in this pixel's direction of the world. The write pass fills every texel the output shows
// before this reads it (the frame, or the edge's continuation where the sphere held nothing), so the smooth fill under
// it only shows through a texel no write reached. Read as a cubic B-spline over 4 x 4 texels (smooth, no ringing, all
// its weights positive, so empty texels just drop out)
vec4 sphere_fill(vec2 uv)
{
    vec4 base = edge_fill(uv);
    if (!frame_setup()) return base;
    vec2 tc = sphere_coord(sphere_world(cur_j)) - 0.5;
    ivec2 size = imageSize(GYROFLOW_SPHERE);
    vec2 c = floor(tc), f = tc - c;
    vec2 f2 = f * f, f3 = f2 * f;
    vec2 w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0, w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    vec2 w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0, w3 = f3 / 6.0;
    vec4 wx = vec4(w0.x, w1.x, w2.x, w3.x), wy = vec4(w0.y, w1.y, w2.y, w3.y);
    ivec2 i0 = ivec2(c) - 1;
    vec3 sum = vec3(0.0);
    float ws = 0.0;
    for (int y = 0; y < 4; y++) {
        int py = clamp(i0.y + y, 0, size.y - 1);
        for (int x = 0; x < 4; x++) {
            vec4 s = imageLoad(GYROFLOW_SPHERE, ivec2((i0.x + x + size.x) % size.x, py));
            float k = sphere_written(s.a) ? wx[x] * wy[y] : 0.0;
            if (k > 0.0) {
                sum += k * s.rgb;
                ws += k;
            }
        }
    }
    if (ws <= 1e-6) return base;
    // Fully the sphere's once two thirds of the taps' weight is written (a texel no write reached doesn't show as a hole)
    return vec4(mix(base.rgb, sum / ws, clamp(ws * 1.5, 0.0, 1.0)), base.a);
}
#endif
#endif

#ifndef SPHERE_PASS
// The fill where the frame doesn't cover the output: the sphere over the smooth fill, or the smooth fill. Kept for the
// pixel: under water finish_rgb asks for it again for red and blue at the same place
bool fill_kept = false;
vec4 fill_value;
vec4 fill_at(vec2 uv)
{
    if (fill_kept) return fill_value;
#if SPHERE_FILL
    fill_value = sphere_fill(uv);
#elif EDGE_FILL
    fill_value = edge_fill(uv);
#else
    fill_value = BACKGROUND;
#endif
    fill_kept = true;
    return fill_value;
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
#endif

#ifdef GRID_STEP
// The grid: point (i, g) at output pixel ((i, g) - 1) * GRID_STEP, one before the edge to two past it
ivec2 grid_size() { return ivec2(ceil(RT / float(GRID_STEP))) + 3; }

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
    vec2 j = HOOKED_pos * RT - 0.5;
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
    vec2 j = HOOKED_pos * RT - 0.5;
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
            vec2 j = (cell + (vec2(t % 3, t / 3) + 0.5) / 3.0) * RT / vec2(FILL_W, FILL_H) - 0.5;
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
// The sphere's four passes, at MAIN before the warp samples the frame (so what they write shows in this very frame;
// HOOKED is the frame as decoded, in RGB). gpu_table.rs defines SPHERE_EDGES, SPHERE_SECTORS_PASS, SPHERE_WRITE or
// SPHERE_BEYOND for each. The rule was worked out on a model first (tools/edgefill/synth, 2026-11-19: its MEMORY=split,
// SW=512, the hard write and REACH=antipode), and the shader is checked against it there.
//
// What the sphere holds (Anton's model): the frame's trace where the frame has been, and the frame's edge continued (the
// smooth fill's colors) where it hasn't. Nothing fades with time: a texel keeps what was written there until the frame
// or a new continuation is written over it. The output always shows the sphere past the frame. The sphere is coarse
// (512 x 256, 0.7 degrees a texel) and read back smooth (a B-spline), which is what softens a trace's border.
// - Inside the frame: the frame, wholly (averaged over the texel), but for its outermost SPHERE_SOFT_IN of the height
//   (the interpolation's taps there reach past the edge) easing in to SPHERE_SOFT. A wide soft edge had weakened what
//   only the frame's outer part saw and left a frozen soft band where a shaking frame came to rest.
// - Past the frame, where the sphere holds nothing (the first frame, a seek, a turn to where the camera hasn't
//   looked): the continuation, wholly.
// - Past the frame, where it holds something: the continuation only where the world has changed at that part of the
//   edge. The frame's edge, all the way round, in SPHERE_SECTORS sectors; per sector, in a band of SPHERE_BAND of the
//   height inside the edge, the frame now against the sphere's memory of the same directions (its slow layer,
//   GYROFLOW_SPHERE_SLOW). The memory is kept apart for what the camera saw and what only a continuation put there:
//   a texel the camera has seen keeps what it saw in the slow layer (continuations written over it don't move it), so a
//   shaking frame whose edge keeps leaving and coming back finds the same memory and wipes nothing; only the world
//   changed (a cut, surfacing, the light) makes it differ. Texels that only ever held a continuation are compared on
//   their own, and they alone are rewritten when the frame shows the continuation was wrong.
//   Where a sector differs, the continuation is written over the whole wedge of the sphere past it: everything around
//   the frame's axis in that direction, out to the point behind the camera, the parts the output doesn't show too
//   (pass sphere_beyond), a share of the way each frame (half of it in SPHERE_TAKE). A trace is worth something only
//   while it joins the frame; one past a changed edge is a picture of a world that isn't there any more.
// The difference: the distance between the two averages in Y'CbCr of the signal as decoded, chroma counted
// SPHERE_CHROMA times; from SPHERE_LO nothing is written, at SPHERE_HI the whole continuation, smoothstep between. Each
// sector's distance is smoothed over time (half-life SPHERE_RISE going up, SPHERE_FALL going down), and the strength
// between sector middles is interpolated along the edge. The sums are pass sphere's, per texel as the sphere holds it,
// so the strengths a frame writes with are the frame before's (a frame late against a 0.15 s smoothing). A seek (or the first frame) starts the sphere over: texels
// written before it count as empty (ge_epoch). SPHERE_DEBUG 1 writes the continuation red over traces, green where the
// sphere held nothing, blue in the wedges past the output
#define SPHERE_BAND 0.03
#define SPHERE_CHROMA 2.0
#define SPHERE_LO 0.04
#define SPHERE_HI 0.10
#define SPHERE_RISE 0.15
#define SPHERE_FALL 0.3
#define SPHERE_SLOW 0.5
#define SPHERE_TAKE 0.08
#define SPHERE_SOFT 0.012
#define SPHERE_SOFT_IN 0.004
#define SPHERE_SUMS 16u
#define SPHERE_WEDGES 64
#ifndef SPHERE_DEBUG
#define SPHERE_DEBUG 0
#endif
#define SPHERE_MAGIC 0x5e3d1a7cu

// Whether a texel written at time t holds something of this run (since the last seek; not after now)
bool sphere_known(float t) { return t >= ge_epoch - 0.001 && t <= PTS + 1000.05; }

// The slow layer's alpha: 0 where the texel only ever held a continuation; 1 + the frame's share it has had (the soft
// edge's: written again while the camera holds still, it mustn't add up) where the camera has seen it
bool slow_seen(float a) { return a >= 0.5; }
float slow_share(float a) { return a >= 0.5 ? clamp(a - 1.0, 0.0, 1.0) : 0.0; }
float slow_alpha(bool seen, float w) { return seen ? 1.0 + clamp(w, 0.0, 1.0) : 0.0; }

// Output pixel j's place in the frame, the grid's, else where the lens's field ends (boundary_uv); past the field,
// beyond_d: how far past it, in source pixels (the focal length's worth a radian), to add to the distance from the frame
float beyond_d;
bool sphere_uv(vec2 j, out vec2 uv)
{
    beyond_d = 0.0;
    if (grid_coord(j, uv)) return true;
    want_boundary = true;
    bool ok = source_coord(j, uv);
    want_boundary = false;
    if (!ok && boundary_uv) beyond_d = max(boundary_excess, 0.0) * abs(fc.x) + 1.0;
    return ok || boundary_uv;
}

// The place along the frame's edge nearest to uv, 0 .. 1 clockwise from the top left corner (its share of the
// perimeter), and how far in from the edge uv is (in_d, source pixels; 0 outside)
float in_d;
float edge_place(vec2 uv)
{
    vec2 lo = vec2(-0.5), hi = vec2(SRC_W, SRC_H) - 0.5;
    vec2 e = clamp(uv, lo, hi);
    float dl = e.x - lo.x, dr = hi.x - e.x, dt = e.y - lo.y, db = hi.y - e.y;
    float w = float(SRC_W), h = float(SRC_H);
    in_d = min(min(dl, dr), min(dt, db));
    float p;
    if (in_d == dt) p = dl;
    else if (in_d == dr) p = w + dt;
    else if (in_d == db) p = w + h + dr;
    else p = 2.0 * w + h + db;
    return p / (2.0 * (w + h));
}

// A direction of the world's angle around the frame's axis (the middle row's camera), -pi .. pi, and the place on the
// frame's edge that angle points to from the frame's middle (wedge_place): the wedge of the sphere a sector heads
float wedge_place;
float wedge_angle(vec3 w)
{
    float rows = t2.z;
    vec4 v = table(15);
    vec3 p = qrot(row_rotation(rows > 1.0 ? int(rows) / 2 : 0, rows), qrot(vec4(-v.xyz, v.w), w));
    float phi = atan(p.y, p.x);
    vec2 d = vec2(cos(phi), sin(phi)), h = 0.5 * vec2(SRC_W, SRC_H);
    float t = 0.999 * min(h.x / max(abs(d.x), 1e-9), h.y / max(abs(d.y), 1e-9));
    wedge_place = edge_place(h + t * d);
    return phi;
}

// The decoded frame at source coordinate uv, bilinear (a compute pass: no implicit level of detail)
vec3 sphere_src(vec2 uv) { return (textureLod(HOOKED_raw, (texel(uv) + 0.5) * HOOKED_pt, 0.0) * HOOKED_mul).rgb; }

// How different two colors look: Y'CbCr (BT.709's weights) distance, chroma counted SPHERE_CHROMA times
float edge_distance(vec3 a, vec3 b)
{
    vec3 d = a - b;
    float y = dot(d, vec3(0.2126, 0.7152, 0.0722));
    return length(vec3(y, SPHERE_CHROMA * (d.b - y) / 1.8556, SPHERE_CHROMA * (d.r - y) / 1.5748));
}

#ifdef SPHERE_EDGES
// The output's border (every fourth pixel along it): the continuation there per angle around the frame's axis, summed
// (fixed point, 1/1024): the color the wedges past the output get. (The sectors' sums are pass sphere's: it knows each
// texel inside the frame's edge as the sphere has it, the frame averaged over the texel and its memory; compared at a
// pixel, a thin bright post against a 0.7 degree texel's average read as a change)
void hook()
{
    vec2 j = vec2(gl_GlobalInvocationID.xy) * 4.0 + 1.5;
    if (any(greaterThanEqual(j, RT)) || !frame_setup()) return;
    vec2 last = (ceil((RT - 1.5) / 4.0) - 1.0) * 4.0 + 1.5;
    if (!(j.x == 1.5 || j.y == 1.5 || j.x == last.x || j.y == last.y)) return;
    cur_j = j;
    uvec3 c = uvec3(clamp(edge_fill(vec2(0.0)).rgb, 0.0, 1.0) * 1024.0 + 0.5);
    float phi = wedge_angle(sphere_world(j));
    uint b = (uint(floor((phi + PI) / (2.0 * PI) * float(SPHERE_WEDGES))) % uint(SPHERE_WEDGES)) * 4u;
    atomicAdd(ge_wsum[b], c.r);
    atomicAdd(ge_wsum[b + 1u], c.g);
    atomicAdd(ge_wsum[b + 2u], c.b);
    atomicAdd(ge_wsum[b + 3u], 1u);
}
#endif

#ifdef SPHERE_SECTORS_PASS
// One sector's difference from its sums at k (the frame, how much, the memory), the band having n_in pixels: none
// without memory (nothing to say it's wrong); counted less where the memory covers under a third of the band
float sector_difference(uint k, float n_in)
{
    float f = float(ge_sum[k + 3u]) / 1024.0;
    if (f <= 1e-3) return 0.0;
    vec3 e = vec3(ge_sum[k], ge_sum[k + 1u], ge_sum[k + 2u]) / (1024.0 * f);
    vec3 m = vec3(ge_sum[k + 4u], ge_sum[k + 5u], ge_sum[k + 6u]) / (1024.0 * f);
    return edge_distance(e, m) * clamp(3.0 * f / n_in, 0.0, 1.0);
}

// A difference smoothed over time: up with half-life SPHERE_RISE (a moment's difference doesn't count), down with
// SPHERE_FALL; no reading (d_now < 0: the sector off the band) eases it down
float sector_smooth(float dm, float d_now, float dt, bool jump)
{
    if (jump) return 0.0;
    if (d_now >= 0.0) return dm + (d_now - dm) * (1.0 - exp2(-dt / (d_now > dm ? SPHERE_RISE : SPHERE_FALL)));
    return dm - dm * (1.0 - exp2(-dt / SPHERE_FALL));
}

// One workgroup of SPHERE_WEDGES: each sector's two differences (the seen memory's, the continuations') from the sums,
// smoothed over time, and their strengths; each wedge's color (where the border has none, between the nearest that
// have); the sums cleared for the next frame. The buffer starts as garbage: nothing is trusted until the pass has run
// once (ge_magic). Rendering the same frame again (paused, a redraw) changes nothing; a seek starts the sphere over
// (ge_epoch) and the smoothing too
shared vec3 s_wc[SPHERE_WEDGES];
shared bool s_wh[SPHERE_WEDGES];
void hook()
{
    uint i = gl_LocalInvocationID.x;
    bool first = ge_magic != SPHERE_MAGIC;
    float dt = PTS - ge_pts;
    bool jump = first || dt < -0.001 || dt > 0.5;
    memoryBarrierBuffer();
    barrier();
    for (uint s = i; s < uint(SPHERE_SECTORS); s += uint(SPHERE_WEDGES)) {
        uint k = s * SPHERE_SUMS;
        float n_in = float(ge_sum[k]);
        bool has = !first && n_in >= 4.0;
        float dw = has ? sector_difference(k + 1u, n_in) : -1.0;
        float dc = has ? sector_difference(k + 8u, n_in) : -1.0;
        ge_d[s] = sector_smooth(first ? 0.0 : ge_d[s], dw, dt, jump);
        ge_dc[s] = sector_smooth(first ? 0.0 : ge_dc[s], dc, dt, jump);
        ge_s[s] = smoothstep(SPHERE_LO, SPHERE_HI, ge_d[s]);
        ge_sc[s] = smoothstep(SPHERE_LO, SPHERE_HI, ge_dc[s]);
        for (uint q = 0u; q < SPHERE_SUMS; q++) ge_sum[k + q] = 0u;
    }
    if (i < uint(SPHERE_WEDGES)) {
        uint b = i * 4u;
        float n = float(ge_wsum[b + 3u]);
        s_wh[i] = n > 0.0;
        s_wc[i] = n > 0.0 ? vec3(ge_wsum[b], ge_wsum[b + 1u], ge_wsum[b + 2u]) / (1024.0 * n) : vec3(0.0);
        for (uint q = 0u; q < 4u; q++) ge_wsum[b + q] = 0u;
    }
    barrier();
    if (i < uint(SPHERE_WEDGES)) {
        int b = int(i);
        vec3 c = s_wc[b];
        bool ok = s_wh[b];
        if (!ok) {
            int lo = 0, hi = 0;
            for (int d = 1; d < SPHERE_WEDGES; d++) {
                if (lo == 0 && s_wh[(b - d + SPHERE_WEDGES) % SPHERE_WEDGES]) lo = d;
                if (hi == 0 && s_wh[(b + d) % SPHERE_WEDGES]) hi = d;
            }
            ok = lo > 0 && hi > 0;
            if (ok) c = mix(s_wc[(b - lo + SPHERE_WEDGES) % SPHERE_WEDGES], s_wc[(b + hi) % SPHERE_WEDGES], float(lo) / float(lo + hi));
        }
        if (ok) {
            ge_w[b * 3] = c.r;
            ge_w[b * 3 + 1] = c.g;
            ge_w[b * 3 + 2] = c.b;
        }
    }
    memoryBarrierBuffer();
    barrier();
    if (i == 0u) {
        // The time since the frame before, for the write passes' shares (none when the same frame is drawn again)
        ge_dt = jump ? 0.0 : max(dt, 0.0);
        ge_pts = PTS;
        if (jump) ge_epoch = PTS + 1000.0;
        ge_magic = SPHERE_MAGIC;
    }
}
#endif

#if defined(SPHERE_WRITE) || defined(SPHERE_BEYOND)
// The sector strength at a place along the edge, between the two nearest sectors' middles: the seen memory's for a
// texel the camera has seen, else the larger of it and the continuations'
float sphere_strength(float place, bool seen)
{
    if (ge_magic != SPHERE_MAGIC) return 0.0;
    float x = place * float(SPHERE_SECTORS) - 0.5;
    float c = floor(x);
    int a = (int(c) + SPHERE_SECTORS) % SPHERE_SECTORS, b = (a + 1) % SPHERE_SECTORS;
    float s = mix(ge_s[a], ge_s[b], x - c);
    return seen ? s : max(s, mix(ge_sc[a], ge_sc[b], x - c));
}
#endif

#ifdef SPHERE_WRITE
// Every fourth output pixel each way (a texel of the sphere is several of a 1080p frame's), and each texel of the
// sphere these pixels reach written by one of them: the one nearest the texel's middle in steps of the pixels (their
// lattice mapped onto the sphere by its local derivatives, from the neighbours' places in the workgroup), whether the
// pixels are smaller than a texel or bigger (near the poles, a wide view). Every texel reached is stored with this
// frame's time, changed or not, so pass sphere_beyond leaves it alone. The texel's middle decides what it gets: inside
// the frame, the frame's color there (4 x 4 taps over the texel's footprint, a Gaussian of half a texel: the frame
// averaged as the coarse texel sees it, no aliasing) over what the texel held, as SPHERE_SOFT says; past the frame,
// the continuation (the smooth fill's color there): wholly where the texel held nothing, else with the share the
// sector's strength gives it. Texels inside the frame's edge by up to SPHERE_BAND of the height are summed per sector
// for the next frame's sectors
shared vec2 s_tc[64];
shared vec2 s_uv[64];
shared bool s_ok[64];
void hook()
{
    ivec2 l = ivec2(gl_LocalInvocationID.xy);
    int li = l.y * 8 + l.x;
    vec2 j = vec2(gl_GlobalInvocationID.xy) * 4.0;
    if (!frame_setup()) return;
    vec2 uv = vec2(0.0);
    bool ok = sphere_uv(j, uv);
    float beyond = beyond_d;
    ivec2 size = imageSize(GYROFLOW_SPHERE);
    vec2 tc = sphere_coord(sphere_world(j));
    s_tc[li] = tc;
    s_uv[li] = uv;
    s_ok[li] = ok;
    barrier();
    if (!ok || any(greaterThanEqual(j, RT))) return;
    // The lattice's steps on the sphere and in the frame: to the next pixel in the workgroup, or back from the last
    int nx = l.x < 7 ? li + 1 : li - 1, ny = l.y < 7 ? li + 8 : li - 8;
    float sx = l.x < 7 ? 1.0 : -1.0, sy = l.y < 7 ? 1.0 : -1.0;
    vec2 dx = (s_tc[nx] - tc) * sx, dy = (s_tc[ny] - tc) * sy;
    dx.x -= float(size.x) * round(dx.x / float(size.x));
    dy.x -= float(size.x) * round(dy.x / float(size.x));
    mat2 m = mat2(dx, dy);
    if (abs(determinant(m)) < 1e-10) return;
    mat2 inv = inverse(m);
    bool box = s_ok[nx] && s_ok[ny];
    mat2 juv = mat2((s_uv[nx] - uv) * sx, (s_uv[ny] - uv) * sy);
    // A texel's sides in the frame's pixels
    mat2 tuv = juv * inv;
    float now = PTS + 1000.0;
    float slow_k = 1.0 - exp2(-ge_dt / SPHERE_SLOW), take = 1.0 - exp2(-ge_dt / SPHERE_TAKE);
    vec2 lo = vec2(-0.5), hi = vec2(SRC_W, SRC_H) - 0.5;
    ivec2 t0 = ivec2(floor(tc));
    // As many texels each way as a step of the lattice spans (near the sphere's poles, where Gyroflow's world has its
    // y axis, a step spans many texels across), at most 6 across and 2 down
    ivec2 r = clamp(ivec2(ceil(0.5 * (abs(dx) + abs(dy)))) + 1, ivec2(1), ivec2(6, 2));
    for (int y = -r.y; y <= r.y; y++) {
        for (int x = -r.x; x <= r.x; x++) {
            ivec2 t = t0 + ivec2(x, y);
            if (t.y < 0 || t.y >= size.y) continue;
            vec2 k = inv * (tc - (vec2(t) + 0.5));
            if (any(lessThan(k, vec2(-0.5))) || any(greaterThanEqual(k, vec2(0.5)))) continue;
            t.x = (t.x + size.x) % size.x;
            // The texel's middle: in the output (in pixels) and in the frame
            vec2 uc = box ? uv - juv * k : uv;
            cur_j = j - 4.0 * k;
            vec4 old = imageLoad(GYROFLOW_SPHERE, t);
            bool known = sphere_known(old.a);
            vec4 sl = imageLoad(GYROFLOW_SPHERE_SLOW, t);
            bool seen = known && slow_seen(sl.a);
            float w = seen ? slow_share(sl.a) : 0.0;
            vec3 slow = sl.rgb;
            float out_d = length(uc - clamp(uc, lo, hi)) + beyond;
            if (out_d <= 0.0) {
                // The frame's share: none on its outermost pixels (the interpolation's taps there reach past the edge,
                // and some cameras' outermost rows are off in color), whole from SPHERE_SOFT of the height in
                vec2 dn = min(uc - lo, hi - uc);
                float a = smoothstep(SPHERE_SOFT_IN * float(SRC_H) + 1.5, SPHERE_SOFT * float(SRC_H), min(dn.x, dn.y));
                vec3 under = known ? old.rgb : edge_fill(uc).rgb;
                vec3 c = under;
                // Only the share it doesn't have yet: the frame wholly further in; at the edge, more of it only as the
                // edge comes further in
                float share = a >= 0.999 ? 1.0 : a > w ? (a - w) / (1.0 - w) : 0.0;
                bool band = min(dn.x, dn.y) <= SPHERE_BAND * float(SRC_H);
                vec3 f = vec3(0.0);
                if (a > 0.0 || band) {
                    if (box) {
                        vec2 cl = vec2(0.0), ch = vec2(SRC_W, SRC_H) - 1.0;
                        f = vec3(0.0);
                        float ws = 0.0;
                        for (int v = 0; v < 4; v++) {
                            for (int u = 0; u < 4; u++) {
                                vec2 o = vec2(float(u), float(v)) * 0.5 - 0.75;
                                float g = exp(-2.0 * dot(o, o));
                                f += g * sphere_src(clamp(uc + tuv * o, cl, ch));
                                ws += g;
                            }
                        }
                        f /= ws;
                    } else {
                        f = sphere_src(uc);
                    }
                    if (band) {
                        // The sectors' sums for the next frame's pass sphere_sectors (as each texel was before this
                        // frame): how many texels in the band inside the edge; then apart for texels the camera has
                        // seen and texels holding only a continuation, the frame and the memory
                        uint q = uint(min(floor(edge_place(uc) * float(SPHERE_SECTORS)), float(SPHERE_SECTORS - 1))) * SPHERE_SUMS;
                        atomicAdd(ge_sum[q], 1u);
                        if (known) {
                            q += seen ? 1u : 8u;
                            uvec3 fc_ = uvec3(clamp(f, 0.0, 1.0) * 1024.0 + 0.5), m = uvec3(clamp(slow, 0.0, 1.0) * 1024.0 + 0.5);
                            atomicAdd(ge_sum[q], fc_.r);
                            atomicAdd(ge_sum[q + 1u], fc_.g);
                            atomicAdd(ge_sum[q + 2u], fc_.b);
                            atomicAdd(ge_sum[q + 3u], 1024u);
                            atomicAdd(ge_sum[q + 4u], m.r);
                            atomicAdd(ge_sum[q + 5u], m.g);
                            atomicAdd(ge_sum[q + 6u], m.b);
                        }
                    }
                }
                if (a > 0.0) {
                    c = mix(under, f, share);
                    // The slow layer: what the camera saw here over the last second or so (an average, half-life
                    // SPHERE_SLOW), the memory the next frames' frame is checked against; started over where the
                    // camera hadn't seen it
                    slow = seen ? mix(slow, f, slow_k) : f;
                } else if (!known) {
                    slow = c;
                }
                imageStore(GYROFLOW_SPHERE_SLOW, t, vec4(slow, slow_alpha(seen || a > 0.0, max(w, a))));
                imageStore(GYROFLOW_SPHERE, t, vec4(c, now));
                continue;
            }
            // Past the frame: the continuation, wholly where the texel held nothing; over what it held, a share of the
            // way each frame (half of it in SPHERE_TAKE), so a moment's difference barely touches the traces and a
            // lasting one (a cut) replaces them within a few tenths of a second. A seen texel's memory stays what the
            // camera saw
            float a = 1.0;
            if (known) {
                a = take * sphere_strength(edge_place(uc), seen);
                if (a < 1.0 / 512.0) a = 0.0;
            }
            vec3 c = old.rgb;
            if (a > 0.0) {
                vec3 cont = edge_fill(uc).rgb;
#if SPHERE_DEBUG
                // Red: written over traces the sphere held; green: where it held nothing
                cont = known ? vec3(1.0, 0.0, 0.0) : vec3(0.0, 1.0, 0.0);
#endif
                c = known ? mix(old.rgb, cont, a) : cont;
                if (!seen) slow = known ? mix(slow, cont, a) : cont;
            }
            imageStore(GYROFLOW_SPHERE_SLOW, t, vec4(slow, slow_alpha(seen, w * (1.0 - a))));
            imageStore(GYROFLOW_SPHERE, t, vec4(c, now));
        }
    }
}
#endif

#ifdef SPHERE_BEYOND
// Every texel of the sphere pass sphere did not reach this frame (not in the output): where its wedge's sector says
// the world changed, the wedge's color (the continuation where that angle leaves the output, from pass sphere_sectors),
// a share of the way each frame as past the frame in the output
void hook()
{
    ivec2 t = ivec2(gl_GlobalInvocationID.xy);
    ivec2 size = imageSize(GYROFLOW_SPHERE);
    if (any(greaterThanEqual(t, size)) || !frame_setup() || ge_magic != SPHERE_MAGIC) return;
    float take = 1.0 - exp2(-ge_dt / SPHERE_TAKE);
    if (take <= 0.0) return;
    vec4 old = imageLoad(GYROFLOW_SPHERE, t);
    float now = PTS + 1000.0;
    if (!sphere_known(old.a) || abs(old.a - now) < 1e-3) return;
    vec2 ll = ((vec2(t) + 0.5) / vec2(size) - 0.5) * vec2(2.0 * PI, PI);
    float phi = wedge_angle(vec3(cos(ll.y) * sin(ll.x), sin(ll.y), cos(ll.y) * cos(ll.x)));
    vec4 sl = imageLoad(GYROFLOW_SPHERE_SLOW, t);
    bool seen = slow_seen(sl.a);
    float a = take * sphere_strength(wedge_place, seen);
    if (a < 1.0 / 512.0) return;
    float x = (phi + PI) / (2.0 * PI) * float(SPHERE_WEDGES) - 0.5;
    float c = floor(x);
    int b0 = (int(c) + SPHERE_WEDGES) % SPHERE_WEDGES, b1 = (b0 + 1) % SPHERE_WEDGES;
    vec3 col = mix(vec3(ge_w[b0 * 3], ge_w[b0 * 3 + 1], ge_w[b0 * 3 + 2]), vec3(ge_w[b1 * 3], ge_w[b1 * 3 + 1], ge_w[b1 * 3 + 2]), x - c);
#if SPHERE_DEBUG
    col = vec3(0.0, 0.0, 1.0);
#endif
    imageStore(GYROFLOW_SPHERE, t, vec4(mix(old.rgb, col, a), now));
    imageStore(GYROFLOW_SPHERE_SLOW, t, vec4(seen ? sl.rgb : mix(sl.rgb, col, a), slow_alpha(seen, slow_share(sl.a) * (1.0 - a))));
}
#endif

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
// The clip's Stronger (the panel's Transform, for very noisy clips)
bool dn_strong() { return gyroflow_dn_strong > 0; }
ivec2 dn_cell(int s) { return ivec2(s % DN_COLS, s / DN_COLS); }
// The ring holds frames of this render's kind: not after a reset (main.lua's epoch: the camera's mode, a realign) or a
// change of the output's size
bool dn_ok() { return gdn_magic == DN_MAGIC && gdn_epoch == gyroflow_dn_epoch && all(equal(gdn_size, RT)); }
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
// many it has before it and after it, and how many frames before the newest are in a row with it (dn_row). Only frames
// in a row (none missing past a dropped one, none from before a seek)
int dn_nw, dn_kc, dn_nb_back, dn_nb_fwd, dn_row;
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
    dn_row = back;
    dn_kc = min(dn_after(), back);
    dn_nb_fwd = dn_kc;
    dn_nb_back = min(dn_radius(), back - dn_kc);
    // Paused, the frame main.lua asks for (gyroflow_dn_show, its time; -1: none; docs/seek-and-scrub.md): a frame step
    // back shows the frame before from the ring, at once, and while the ring is filled again around a frame (a seek to
    // 5 frames before it, 10 frames played) that frame stays on screen. In the row: with what the row has on each side
    // of it; elsewhere in the ring (the row just broken by that seek): by itself, as it was decoded
    if (gyroflow_dn_show >= 0.0 && ok) {
        float tol = 0.5 / FPS;
        for (int k = 0; k <= 2 * DN_RADIUS; k++) {
            if (abs(pts[k] - gyroflow_dn_show) < tol) {
                bool in_row = k <= back;
                dn_kc = k;
                dn_nb_fwd = in_row ? min(dn_radius(), k) : 0;
                dn_nb_back = in_row ? min(dn_radius(), back - k) : 0;
                break;
            }
        }
    }
}
int dn_center() { return dn_back_slot(dn_kc); }
// The slot o frames after the shown one (o < 0: before it)
int dn_slot_at(int o) { return dn_back_slot(dn_kc - o); }
// Neighbor n of the shown frame: 0 .. DN_RADIUS - 1 the frames before it, nearest first, then those after it
int dn_offset(int n) { return n < DN_RADIUS ? -(n + 1) : n - DN_RADIUS + 1; }
bool dn_has(int n) { int o = dn_offset(n); return o < 0 ? -o <= dn_nb_back : o <= dn_nb_fwd; }

vec3 dn_frame(int s, ivec2 p) { p = clamp(p, ivec2(0), ivec2(RT) - 1); return imageLoad(GDN_FRAMES, p + dn_cell(s) * DN_SLOT).rgb; }
float dn_quart(int s, ivec2 p) { p = clamp(p, ivec2(0), ivec2(RT / 4.0) - 1); return imageLoad(GDN_QUART, p + dn_cell(s) * (DN_SLOT / 4)).x; }
ivec2 dn_blocks() { return ivec2(ceil(RT / 32.0)); }
#endif

// ==== pass dn_store ====
#ifdef DN_STORE
void hook()
{
    // The frame into its slot, as decoded and warped; its quarter-size luma too
    ivec2 j = ivec2(gl_GlobalInvocationID.xy);
    if (any(greaterThanEqual(j, ivec2(RT))) || any(greaterThan(ivec2(RT), DN_SLOT)) || !dn_fresh()) return;
    int s = dn_newest();
    imageStore(GDN_FRAMES, j + dn_cell(s) * DN_SLOT, vec4((HOOKED_mul * texelFetch(HOOKED_raw, j, 0)).rgb, 1.0));
    if (all(equal(j % ivec2(4), ivec2(0))))
        imageStore(GDN_QUART, j / 4 + dn_cell(s) * (DN_SLOT / 4), vec4(texelFetch(GDN_CUR4_raw, j / 4, 0).x * GDN_CUR4_mul));
}
#endif

// ==== pass dn_motion ====
#ifdef DN_MOTION
float cur4(ivec2 p) { return texelFetch(GDN_CUR4_raw, clamp(p, ivec2(0), ivec2(GDN_CUR4_size) - 1), 0).x * GDN_CUR4_mul; }
float cur1(ivec2 p) { return lum(HOOKED_mul * texelFetch(HOOKED_raw, clamp(p, ivec2(0), ivec2(HOOKED_size) - 1), 0).rgb); }

// Where direction d of the output camera (crop mode's camera and zoom left out) is on the output, in this render's
// pixels: output_ray's map the other way
vec2 plane_point(vec3 d, float zoom)
{
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * zoom);
    float rxy = max(length(d.xy), 1e-9);
    float theta = atan(rxy, d.z);
    vec2 u = d.xy / rxy;
    vec2 n = u * proj_radius(theta);
#if LENS_BLEND
    // A lens correction below 100 %: where output_ray's blend puts the ray, (1 - a) of the way to where the source lens
    // images it (under water, refracted first)
    float a = lens.y;
    if (a < 0.999) {
        float th = refracts() ? asin(clamp(sin(theta) * lens.z, -1.0, 1.0)) : theta;
        vec2 src = distort_point(u.x * sin(th), u.y * sin(th), cos(th));
        if (src.x > -9999.0 && src.y > -9999.0) n = a > 0.0 ? (1.0 - a) * src + a * n : src;
    }
#endif
    return (n * out_f + 0.5 * size - t2.xy) * RT / size;
}

// Where output pixel j was in the frame before (its camera q; crop mode's camera cam and zoom as it was rendered) by
// the gyro alone: its ray turned into the world by this frame's camera, out of it by that one's. As a move from j, both
// ends through plane_point, so what it leaves out (a digital lens; output_ray's solve, within its tolerance) is about the
// same at both and cancels. It used to be the rectilinear map alone, wrong wherever the lens correction is below 100 %
// (old GoPro clips, Action 6 at 0 %): at 0 % on 1080p up to 180 px at the sides (GoPro HERO3 clip GOPR0605), and the
// search (8 pixels around) never found the motion. Textured places then kept their own frame (no denoise); smooth
// open water took in other places' water, several codes brighter or darker, in rectangles of the blocks (the banding
// and ghosts seen there)
vec2 predict(vec2 j, vec4 qp, vec4 cam, float zoom)
{
    vec3 r = output_ray(j);
    vec3 d0 = qrot(vec4(-gyroflow_cam_x, -gyroflow_cam_y, -gyroflow_cam_z, gyroflow_cam_w), r);
    vec3 d = qrot(vec4(-cam.xyz, cam.w), qrot(vec4(-qp.xyz, qp.w), qrot(table(15), r)));
    if (d.z < 1e-3 || d0.z < 1e-3) return j;
    return j + plane_point(d, zoom) - plane_point(d0, gyroflow_cam_zoom);
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
    if (dn_row >= 1 && frame_setup()) {
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
// How much a patch may differ beyond the noise and still be averaged in: still.rs's weight's scale (twice it with
// Stronger: more frames taken in where the noise is heavy and blotchy)
#define DN_TOLERANCE (dn_strong() ? 3.0 : 1.5)
vec3 frame_bilinear(int slot, vec2 p)
{
    vec2 f = fract(p);
    ivec2 i = ivec2(floor(p));
    return mix(mix(dn_frame(slot, i), dn_frame(slot, i + ivec2(1, 0)), f.x), mix(dn_frame(slot, i + ivec2(0, 1)), dn_frame(slot, i + ivec2(1, 1)), f.x), f.y);
}

vec4 hook()
{
    ivec2 j = ivec2(HOOKED_pos * HOOKED_size);
    if (any(greaterThan(ivec2(RT), DN_SLOT))) return HOOKED_tex(HOOKED_pos);
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
        // halve it). Only where the 4x4 around it matched that frame within the noise (2.5 times it on the quarter-size
        // luma, whose own noise is about half a pixel's; never under half an 8-bit code): where the motion isn't
        // followed, the difference is the picture's, not its noise. Handheld GoPro HERO4 clip GOPR1030 (8-bit, fast
        // motion, motion blur): over a quarter of the pixels missed, the upper quartile went past the histogram's end
        // and the noise read 9.7 codes for 1.2, so every frame was averaged in, ghosts and other frames' edges with them
        // (lines and steps on skin and walls). Under water (0087) the measure is as it was
        if (n == 0 && abs(eq) < 1.8 * max(2.5 * s, 0.002)) {
            noise_diff = abs(l - lum(near));
            noise_c = abs(chroma(cur).x - chroma(near).x);
        }
    }
    vec3 den = sum / wsum;
    // Stronger: the shown pixel judged too, against what the matched frames agree on. A spike of the noise in it (the
    // dark specks HEVC leaves under water) gets a tenth of the weight instead of staying a part of every frame's own
    if (dn_strong() && wsum >= 3.0) {
        vec3 nbm = (sum - cur) / (wsum - 1.0);
        float ws = mix(0.1, 1.0, 1.0 - smoothstep(2.0 * s, 4.0 * s, abs(l - lum(nbm))));
        den = (sum - cur + ws * cur) / (wsum - 1.0 + ws);
    }
    // The noise, from the shown frame against the one before where they match
    if (!dn_same() && noise_diff >= 0.0 && all(equal(j % ivec2(4), ivec2(0)))) {
        uint bin = uint(clamp(l * 16.0, 0.0, 15.0));
        uint st = uint(clamp(noise_diff * 4096.0, 0.0, 255.0));
        atomicAdd(gdn_hist[bin * 256u + st], 1u);
        atomicAdd(gdn_hist[4096u + bin * 256u + uint(clamp(noise_c * 4096.0, 0.0, 255.0))], 1u);
    }
    // Some of the grain kept, so it doesn't look smeared (gyroflow.conf's still_grain); none with Stronger
    return vec4(den + (dn_strong() ? 0.0 : gyroflow_dn_keep) * (cur - den), 1.0);
}
#endif

// ==== pass dn_clean ====
// After the frames are averaged, the frame itself (in both the denoise's modes): what averaging frames can't take out.
// - Dark specks: HEVC under water leaves spikes of the noise, mostly in red (dark cyan dots on blue water, 1-7 px), new
//   each frame; the frame's own pixel keeps part of them. Where a pixel is darker than the ring of 16 pixels two away by
//   more than the noise, and that ring has no dark pixels opposite each other (a dark line or edge through it would),
//   it takes the ring's color. Bright dots stay: particles in the water are white. In the filter's last pass.
// - The color: a guided filter (He et al., "fast": its statistics at a quarter of the size) of B-Y and R-Y, each its own
//   guide over ~20 px, so the color's blotches go and an edge of color well above its noise stays (a pink fin over water
//   of its brightness: guided by the luma, its color would spread); with Stronger the luma too (its own guide, ~12 px).
//   Their eps is the noise's (dn_state's measure).
#ifdef DN_GF_LO
// The quarter-size luma and color: each texel the mean of its 4x4 pixels (four bilinear reads)
vec4 hook()
{
    vec2 d = HOOKED_pt;
    vec3 c = 0.25 * (HOOKED_tex(HOOKED_pos + vec2(-d.x, -d.y)).rgb + HOOKED_tex(HOOKED_pos + vec2(d.x, -d.y)).rgb
                   + HOOKED_tex(HOOKED_pos + vec2(-d.x, d.y)).rgb + HOOKED_tex(HOOKED_pos + vec2(d.x, d.y)).rgb);
    float y = lum(c);
    return vec4(y, c.b - y, c.r - y, 1.0);
}
#endif
#if defined(DN_GF_AY) || defined(DN_GF_AC)
// The filter's a and b per quarter texel, from the means over a window around it: the luma's (1 texel around: ~12 px)
// and the color's (2 around: ~20 px), each its own guide
vec4 hook()
{
#ifdef DN_GF_AY
    const int R = 1;
#else
    const int R = 2;
#endif
    vec2 d = GDN_GF_LO_pt;
    vec2 p = GDN_GF_LO_pos;
    float my = 0.0, myy = 0.0;
    vec2 mc = vec2(0.0), mcc = vec2(0.0);
    for (int dy = -R; dy <= R; dy++)
        for (int dx = -R; dx <= R; dx++) {
            vec3 v = GDN_GF_LO_tex(p + vec2(dx, dy) * d).xyz;
            my += v.x; myy += v.x * v.x; mc += v.yz; mcc += v.yz * v.yz;
        }
    float n = float((2 * R + 1) * (2 * R + 1));
    my /= n; myy /= n; mc /= n; mcc /= n;
#ifdef DN_GF_AY
    float var = max(myy - my * my, 0.0);
    float e = 0.4 * sigma_at(my);
    float a = var / (var + e * e);
    return vec4(a, my - a * my, 0.0, 1.0);
#else
    vec2 var = max(mcc - mc * mc, vec2(0.0));
    float e = 0.8 * sigc_at(my);
    vec2 a = var / (var + e * e);
    return vec4(a.x, mc.x - a.x * mc.x, a.y, mc.y - a.y * mc.y);
#endif
}
#endif
#if defined(DN_GF_MY) || defined(DN_GF_MC)
// The means of a and b over the same windows
vec4 hook()
{
#ifdef DN_GF_MY
    const int R = 1;
    vec2 d = GDN_GF_AY_pt;
    vec2 p = GDN_GF_AY_pos;
#else
    const int R = 2;
    vec2 d = GDN_GF_AC_pt;
    vec2 p = GDN_GF_AC_pos;
#endif
    vec4 m = vec4(0.0);
    for (int dy = -R; dy <= R; dy++)
        for (int dx = -R; dx <= R; dx++)
#ifdef DN_GF_MY
            m += GDN_GF_AY_tex(p + vec2(dx, dy) * d);
#else
            m += GDN_GF_AC_tex(p + vec2(dx, dy) * d);
#endif
    return m / float((2 * R + 1) * (2 * R + 1));
}
#endif
#ifdef DN_GF_APPLY
// The ring two pixels away, offset i of 16 (i and 15 - i opposite each other)
vec2 ring_off(int i)
{
    int r = i < 5 ? i : (i < 11 ? (i % 2 == 1 ? 5 + (i - 5) / 2 * 5 : 9 + (i - 6) / 2 * 5) : 20 + (i - 11));
    return vec2(r % 5 - 2, r / 5 - 2);
}
// The pixel without a dark speck: c, or its ring's color where c is one
vec3 despeck(vec3 c)
{
    float yc = lum(c);
    float t = (dn_strong() ? 1.0 : 1.3) * max(sigma_at(yc), 1e-4);
    // Most pixels aren't darker than the four pixels two away: done at four reads
    float m4 = 0.25 * (lum(HOOKED_texOff(vec2(2.0, 0.0)).rgb) + lum(HOOKED_texOff(vec2(-2.0, 0.0)).rgb)
                     + lum(HOOKED_texOff(vec2(0.0, 2.0)).rgb) + lum(HOOKED_texOff(vec2(0.0, -2.0)).rgb));
    if (yc >= m4 - 0.5 * t) return c;
    float m0 = 0.0;
    for (int i = 0; i < 16; i++) m0 += lum(HOOKED_texOff(ring_off(i)).rgb);
    m0 /= 16.0;
    // The ring's level without its own dark ones
    float m1 = 0.0, n1 = 0.0;
    vec3 c1 = vec3(0.0);
    for (int i = 0; i < 16; i++) {
        vec3 v = HOOKED_texOff(ring_off(i)).rgb;
        if (lum(v) > m0 - t) { m1 += lum(v); c1 += v; n1 += 1.0; }
    }
    if (n1 < 8.0 || yc >= m1 / n1 - t) return c;
    m1 /= n1;
    // Dark on both sides: a line or an edge through it, kept
    for (int i = 0; i < 8; i++)
        if (lum(HOOKED_texOff(ring_off(i)).rgb) < m1 - t && lum(HOOKED_texOff(ring_off(15 - i)).rgb) < m1 - t) return c;
    return c1 / n1;
}
// q = a I + b at full size, a and b between the quarter texels (the sampler's filter), I the pixel's own
vec4 hook()
{
    // The dark specks first (in this pass, not one of their own: a full-size pass costs ~0.4 ms; the filter's statistics,
    // from before, see them, few as they are)
    vec3 c = despeck(HOOKED_texOff(vec2(0.0)).rgb);
    float y = lum(c);
    vec4 ac = GDN_GF_MC_tex(HOOKED_pos);
    vec2 ay = GDN_GF_MY_tex(HOOKED_pos).xy;
    float cb = ac.x * (c.b - y) + ac.y, cr = ac.z * (c.r - y) + ac.w;
    if (dn_strong()) y = ay.x * y + ay.y;
    // Back to RGB from the luma and B-Y, R-Y (lum's weights)
    float r = cr + y, b = cb + y;
    float g = (y - 0.2627 * r - 0.0593 * b) / 0.678;
    return vec4(r, g, b, 1.0);
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
        // taken for motion. Only the pixels whose surroundings matched (dn_blend), else on a fast handheld clip the
        // misses are the upper quartile, the noise reads several times too high and other frames' edges are averaged in.
        // Smoothed over the frames (a tenth of each); a bin with too few keeps what it had, and
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
        gdn_size = RT;
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
