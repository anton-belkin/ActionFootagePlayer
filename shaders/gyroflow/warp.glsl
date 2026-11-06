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
// What the spike covers: the opencv_fisheye lens, full lens correction, no digital lens, the output projections but
// the equirect, rolling shutter (the two-step row lookup), refraction under water, a solid background. The
// interpolation is chosen per file (INTERP): 0 bilinear, 1 B-spline (4 bilinear fetches), 2 Gyroflow's bicubic (Keys
// a = -0.75, 16 taps), 3 Lanczos4 (64 taps), 4 EWA Robidoux (Gyroflow's, up to 9x9 taps). SHRINK 1 widens the
// bicubic's and Lanczos's taps where the warp shrinks the source (up to 4x, each tap a bilinear fetch).
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

//!HOOK MAIN
//!BIND HOOKED
//!BIND GYROFLOW_TABLE
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!WHEN gyroflow_gpu_on
//!DESC gyroflow warp

@DEFINES@

// The table: frame i's block of BLOCK texels at (i % COLS) * BLOCK, row i / COLS. Texels 0-4 the frame's parameters,
// then KNOTS quaternions (x, y, z, w) of the row rotations, the knots evenly over the rows (FULL_ROWS: one per row)
//   0: f.x, f.y, c.x, c.y          1: k0, k1, k2, k3
//   2: fov, lens correction, refraction coefficient, field limit
//   3: translation2d.x, .y, matrix rows (1: no rolling shutter), valid (1)
//   4: translation3d.x, .y, .z, 0

#define PI 3.14159265358979

int table_frame;

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
    return table(5 + y);
#else
    float t = rows > 1.0 ? float(y) / (rows - 1.0) * float(KNOTS - 1) : 0.0;
    int j = min(int(t), KNOTS - 2);
    vec4 q = mix(table(5 + j), table(6 + j), t - float(j));
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

// opencv_fisheye's distort_point
vec2 distort(vec3 p, vec4 k)
{
    if (all(equal(k, vec4(0.0))))
        return p.z > 1e-9 ? p.xy / p.z : p.xy * 1e9;
    float r = length(p.xy);
    float theta = atan(r, p.z);
    float t2 = theta * theta;
    float t4 = t2 * t2;
    float theta_d = theta * (1.0 + k.x * t2 + k.y * t4 + k.z * t4 * t2 + k.w * t4 * t4);
    float scale = r < 1e-12 ? 1.0 : theta_d / r;
    return p.xy * scale;
}

// Stabilization::rotate_and_distort: a ray in the output camera, turned into the source camera by row rotation q,
// imaged by the lens. False where the ray has no image (past the field limit, outside Snell's window)
bool rotate_and_distort(vec3 d, vec4 q, vec4 fc, vec4 k, vec4 lens, vec3 t3, out vec2 uv)
{
    vec3 p = qrot(q, d) + t3;
    float rxy = length(p.xy);
    float theta = atan(rxy, p.z);
    uv = vec2(0.0);
    if (lens.w > 0.0 && theta > lens.w) return false;
    float refr = lens.z;
    if (refr != 1.0 && refr > 0.0 && rxy > 1e-12) {
        float sin_d = sin(theta) * refr;
        if (sin_d >= 1.0 || p.z <= 0.0) return false;
        float theta_d = asin(sin_d);
        p = vec3(p.xy * (sin(theta_d) / rxy), cos(theta_d));
    }
    uv = distort(p, k) * fc.xy + fc.zw;
    return true;
}

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
#else
vec4 sample_src(vec2 uv) { return src(uv); }
#endif

vec4 hook()
{
    table_frame = clamp(int(floor(PTS * FPS + 0.5)), 0, FRAMES - 1);
    vec4 fc = table(0);
    vec4 k = table(1);
    vec4 lens = table(2);
    vec4 t2 = table(3);
    vec3 t3 = table(4).xyz;
    if (t2.w < 0.5) return BACKGROUND;

    // The output pixel in the kernel's output frame (OUT_W x OUT_H), as the VapourSynth path maps its pixel j there:
    // j * OUT_W / width
    vec2 size = vec2(OUT_W, OUT_H);
    vec2 out_pos = HOOKED_pos * size - 0.5 * size / target_size + t2.xy;
    vec2 out_f = fc.xy / lens.x;
    vec3 d = ray_to_dir(unproject((out_pos - 0.5 * size) / out_f));

    // The rolling shutter's row: where the middle row's rotation puts the ray, then that row's rotation
    float rows = t2.z;
    vec2 uv;
    int idx = 0;
    if (rows > 1.0) {
        int mid = int(rows) / 2;
        if (rotate_and_distort(d, row_rotation(mid, rows), fc, k, lens, t3, uv))
            idx = clamp(int(floor(uv.y + 0.5)), 0, SRC_H);
        idx = min(idx, int(rows) - 1);
    }
    if (!rotate_and_distort(d, row_rotation(idx, rows), fc, k, lens, t3, uv))
        return BACKGROUND;
    // Past the frame by more than half a pixel: the background, as Gyroflow's taps outside it are
    if (any(lessThan(uv, vec2(-0.5))) || any(greaterThan(uv, vec2(SRC_W, SRC_H) - 0.5)))
        return BACKGROUND;
#ifdef UV_OUT
    // The tests' output (gpu_table.rs): the source coordinate, not the picture
    return vec4(uv, 0.0, 1.0);
#else
    // Gyroflow's frame -> the texture's pixels (the same frame unless mpv hands over another size)
    uv = (uv + 0.5) * HOOKED_size / vec2(SRC_W, SRC_H) - 0.5;
#ifdef PLANE_OFFSET
    // A plane read on its own (the spike's LUMA and CHROMA passes): where its samples sit against the frame's, in its
    // pixels (MPEG-2's left-sited chroma: +0.25 across)
    uv += PLANE_OFFSET;
#endif

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
