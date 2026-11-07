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
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * gyroflow_cam_zoom);
    vec3 d = ray_to_dir(unproject((out_pos - 0.5 * size) / out_f));
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

// The output pixel being drawn (the sphere trail's direction)
vec2 cur_j;

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

vec4 finish(vec2 uv)
{
    if (outside(uv)) {
#ifdef UV_OUT
        return BACKGROUND;
#elif SPHERE_FILL
        return sphere_fill(uv);
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
    cur_j = j;
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
// Temporal denoise (the clean frame while it plays: Ctrl+p), after the warp at MAIN, in RGB. The paused frame's way
// (vs-gyroflow/src/still.rs), with the frames before it only: each pixel is the average of it and the last DN_PAST
// frames' pixels where the motion found between them puts them, each weighted by how much the patches around them
// differ beyond the noise. Where something moved in a way the motion doesn't follow, the patches differ and the frame
// keeps its own pixel: no trails, and nothing carried over from frame to frame. The motion: per 32 px block and past
// frame, the gyro's prediction (the smoothed camera's turn between them, what the stabilized picture moves by), a search
// around it on a quarter-size luma, then to the pixel at full size. The noise: by brightness, from how the frame differs
// from the one before where they match. The frames, as decoded and warped, in a ring of DN_RING slots in one texture
// (GDN_FRAMES, slots one under the other) and their quarter-size luma in another (GDN_QUART), which keeps the storage
// images per pass few (MoltenVK's limit). This pass: the frame's luma at a quarter of its size, for the search
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
#define DN_QSLOT ivec2(DN_SLOT_W / 4, DN_SLOT_H / 4)
float lum(vec3 c) { return dot(c, vec3(0.2627, 0.678, 0.0593)); }
// This render's frame is new (else the same frame drawn again: paused, a redraw), and where its past frames are
bool dn_same() { return gdn_magic == 0x5E1Fu && abs(PTS - gdn_pts[gdn_head]) < 1e-4; }
// The slot of the k-th frame before this one (k = 0: the one just before)
int dn_slot(int k) { int base = dn_same() ? gdn_head - 1 : gdn_head; return (base - k + 2 * DN_RING) % DN_RING; }
bool dn_valid(int k)
{
    if (gdn_magic != 0x5E1Fu) return false;
    int s = dn_slot(k);
    float dt = PTS - gdn_pts[s];
    return dt > 1e-4 && dt < (float(k) + 1.6) / FPS;
}
vec3 dn_frame(int slot, ivec2 p) { p = clamp(p, ivec2(0), ivec2(target_size) - 1); return imageLoad(GDN_FRAMES, p + ivec2(0, slot * DN_SLOT_H)).rgb; }
float dn_quart(int slot, ivec2 p) { p = clamp(p, ivec2(0), ivec2(target_size / 4.0) - 1); return imageLoad(GDN_QUART, p + ivec2(0, slot * (DN_SLOT_H / 4))).x; }
#endif

// ==== pass dn_motion ====
#ifdef DN_MOTION
float cur4(ivec2 p) { return texelFetch(GDN_CUR4_raw, clamp(p, ivec2(0), ivec2(GDN_CUR4_size) - 1), 0).x * GDN_CUR4_mul; }
float cur1(ivec2 p) { return lum(HOOKED_mul * texelFetch(HOOKED_raw, clamp(p, ivec2(0), ivec2(HOOKED_size) - 1), 0).rgb); }

// Where output pixel j was in a past frame (its camera q) by the gyro alone: its ray turned into the world by this
// frame's camera, out of it by that one's
vec2 predict(vec2 j, vec4 qp)
{
    vec3 w = qrot(table(15), output_ray(j));
    // Out of the world by that frame's camera, then crop mode's (output_ray's turn, undone)
    vec3 d = qrot(vec4(-gyroflow_cam_x, -gyroflow_cam_y, -gyroflow_cam_z, gyroflow_cam_w), qrot(vec4(-qp.xyz, qp.w), w));
    if (d.z < 1e-3) return j;
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_f = fc.xy * vec2(stretch.x > 0.01 ? 1.0 / stretch.x : 1.0, 1.0) / (lens.x * gyroflow_cam_zoom);
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
    // x: the block across, y: the block down and the past frame (blocks of frame k below those of k - 1)
    ivec2 nb = ivec2(ceil(target_size / 32.0));
    ivec2 g = ivec2(gl_GlobalInvocationID.xy);
    int k = g.y / nb.y;
    ivec2 b = ivec2(g.x, g.y % nb.y);
    if (g.x >= nb.x || k >= DN_PAST) return;
    vec4 res = vec4(0.0, 0.0, 1e9, 0.0);
    if (dn_valid(k) && frame_setup()) {
        int slot = dn_slot(k);
        vec2 c = (vec2(b) + 0.5) * 32.0;
        vec2 pred = predict(c, gdn_quat[slot]) - c;
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
        // Full size: within 1 px of it, every fourth pixel of the 32x32 block
        ivec2 o1 = b * 32;
        ivec2 base = bv * 4;
        float c1[64];
        for (int t = 0; t < 64; t++) c1[t] = cur1(o1 + 4 * ivec2(t % 8, t / 8));
        float best1 = 1e9;
        ivec2 bv1 = base;
        for (int dy = -1; dy <= 1; dy++) {
            for (int dx = -1; dx <= 1; dx++) {
                ivec2 cand = base + ivec2(dx, dy);
                float sad = 0.0;
                for (int t = 0; t < 64; t++) sad += abs(c1[t] - lum(dn_frame(slot, o1 + 4 * ivec2(t % 8, t / 8) + cand)));
                if (dx == 0 && dy == 0) sad *= 0.9;
                if (sad < best1) { best1 = sad; bv1 = cand; }
            }
        }
        res = vec4(vec2(bv1), best1 / 64.0, 1.0);
    }
    imageStore(GDN_VEC, g, res);
}
#endif

// ==== pass dn_blend ====
#ifdef DN_BLEND
float sigma_at(float l)
{
    float b = clamp(l * 16.0 - 0.5, 0.0, 15.0);
    int i = int(b);
    return mix(gdn_sigma[i], gdn_sigma[min(i + 1, 15)], b - float(i));
}
vec3 frame_bilinear(int slot, vec2 p)
{
    vec2 f = fract(p);
    ivec2 i = ivec2(floor(p));
    return mix(mix(dn_frame(slot, i), dn_frame(slot, i + ivec2(1, 0)), f.x), mix(dn_frame(slot, i + ivec2(0, 1)), dn_frame(slot, i + ivec2(1, 1)), f.x), f.y);
}

vec4 hook()
{
    ivec2 j = ivec2(HOOKED_pos * HOOKED_size);
    vec4 cur = HOOKED_tex(HOOKED_pos);
    if (any(greaterThan(ivec2(target_size), DN_SLOT))) return cur;
    bool same = dn_same();
    // A new frame goes into the next slot, as decoded, for the frames after it; its quarter-size luma too
    if (!same) {
        int ns = (gdn_magic == 0x5E1Fu ? gdn_head + 1 : 0) % DN_RING;
        imageStore(GDN_FRAMES, j + ivec2(0, ns * DN_SLOT_H), vec4(cur.rgb, 1.0));
        if (all(equal(j % 4, ivec2(0))))
            imageStore(GDN_QUART, j / 4 + ivec2(0, ns * (DN_SLOT_H / 4)), vec4(texelFetch(GDN_CUR4_raw, j / 4, 0).x * GDN_CUR4_mul));
    }
    ivec2 nb = ivec2(ceil(target_size / 32.0));
    vec2 bp = (vec2(j) + 0.5) / 32.0 - 0.5;
    ivec2 b0 = ivec2(floor(bp));
    vec2 f = bp - vec2(b0);
    // The frame's own patch: 3x3, every other pixel (5x5 pixels), luma
    float own[9];
    for (int t = 0; t < 9; t++)
        own[t] = lum(HOOKED_mul * texelFetch(HOOKED_raw, clamp(j + 2 * ivec2(t % 3 - 1, t / 3 - 1), ivec2(0), ivec2(HOOKED_size) - 1), 0).rgb);
    float l = lum(cur.rgb);
    float s = max(sigma_at(l), 1e-4);
    vec3 sum = cur.rgb;
    float wsum = 1.0;
    float noise_diff = -1.0;
    for (int k = 0; k < DN_PAST; k++) {
        if (!dn_valid(k)) continue;
        int slot = dn_slot(k);
        // The pixel's vector, between the four blocks around it
        ivec2 c00 = clamp(b0, ivec2(0), nb - 1), c11 = clamp(b0 + 1, ivec2(0), nb - 1);
        vec4 v00 = imageLoad(GDN_VEC, ivec2(c00.x, c00.y + k * nb.y)), v10 = imageLoad(GDN_VEC, ivec2(c11.x, c00.y + k * nb.y));
        vec4 v01 = imageLoad(GDN_VEC, ivec2(c00.x, c11.y + k * nb.y)), v11 = imageLoad(GDN_VEC, ivec2(c11.x, c11.y + k * nb.y));
        if (v00.w < 0.5 || v10.w < 0.5 || v01.w < 0.5 || v11.w < 0.5) continue;
        vec2 v = mix(mix(v00.xy, v10.xy, f.x), mix(v01.xy, v11.xy, f.x), f.y);
        float d = 0.0;
        ivec2 pr = ivec2(round(vec2(j) + v));
        for (int t = 0; t < 9; t++) {
            float e = own[t] - lum(dn_frame(slot, pr + 2 * ivec2(t % 3 - 1, t / 3 - 1)));
            d += e * e;
        }
        vec2 p = vec2(j) + v;
        d /= 9.0;
        // still.rs's weight: full where the patches differ by the noise alone, none past three times it
        float w = 1.0 - smoothstep(1.0 * DN_STRENGTH, 3.0 * DN_STRENGTH, d / (2.0 * s * s));
        vec3 h = frame_bilinear(slot, p);
        sum += w * h;
        wsum += w;
        if (k == 0) noise_diff = abs(l - lum(h));
    }
    vec3 den = sum / wsum;
    // The noise, from this frame against the one before where they match
    if (!same && noise_diff >= 0.0 && all(equal(j % 4, ivec2(0)))) {
        uint bin = uint(clamp(l * 16.0, 0.0, 15.0));
        uint st = uint(clamp(noise_diff * 4096.0, 0.0, 255.0));
        atomicAdd(gdn_hist[bin * 256u + st], 1u);
    }
    // Some of the grain kept, so it doesn't look smeared
    return vec4(den + DN_KEEP * (cur.rgb - den), cur.a);
}
#endif

// ==== pass dn_state ====
#ifdef DN_STATE
void hook()
{
    // One workgroup of 16: brightness bin i each
    uint i = gl_LocalInvocationID.x;
    bool same = dn_same();
    memoryBarrierBuffer();
    barrier();
    if (!same) {
        // The noise of bin i: the lower quartile of the differences over 0.4506 (two noisy samples' |a - b| is under it
        // for a quarter of them), as still.rs measures it; a bin with too few keeps what it had
        uint total = 0u;
        for (uint st = 0u; st < 256u; st++) total += gdn_hist[i * 256u + st];
        if (total >= 200u) {
            uint seen = 0u;
            float q = 0.0;
            for (uint st = 0u; st < 256u; st++) {
                uint h = gdn_hist[i * 256u + st];
                if ((seen + h) * 4u >= total) { q = (float(st) + 0.5) / 4096.0; break; }
                seen += h;
            }
            gdn_sigma[i] = max(q / 0.4506, 0.0005);
        } else if (gdn_magic != 0x5E1Fu) {
            gdn_sigma[i] = 0.004;
        }
        for (uint st = 0u; st < 256u; st++) gdn_hist[i * 256u + st] = 0u;
    }
    memoryBarrierBuffer();
    barrier();
    if (i == 0u && !same) {
        // The new frame is in the next slot now (the blend put it there): its time and camera, for the frames after it
        int ns = (gdn_magic == 0x5E1Fu ? gdn_head + 1 : 0) % DN_RING;
        if (gdn_magic != 0x5E1Fu)
            for (int k = 0; k < DN_RING; k++) gdn_pts[k] = -1e9;
        gdn_head = ns;
        gdn_pts[ns] = PTS;
        if (frame_setup()) gdn_quat[ns] = table(15);
        gdn_magic = 0x5E1Fu;
    }
}
#endif
