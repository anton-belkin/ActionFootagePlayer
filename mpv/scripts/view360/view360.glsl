// The 360 view (main.lua; vo=gpu-next, gpu or libmpv): an equirectangular sphere shown as a view to look around in,
// computed for every screen refresh from the frame on screen, so turning the head never waits for the video.
// docs/360-cameras.md, "The view shader's part".
//
// Per output pixel: its ray (the output projection and FOV), turned by the head (yaw, pitch, roll), by the heading of
// the chosen mode and into the camera of this frame (both from the frame's stamp), then looked up in the sphere.
// Conventions are video-types' (as Gyroflow's): x right, y down, z forward; the equirect's longitude 0 at +z, +90° at
// +x (right of the picture's middle), latitude +90° up (its top row).
//
// The head: yaw positive looks right, pitch positive up, roll positive turns the picture counterclockwise; yaw about
// the vertical first, then pitch about the head's own horizontal axis, then roll about where it looks (as crop mode's
// camera on its tripod). The FOV is what a 16:9 picture of the window's height spans across: the scale stays as the
// window gets wider or narrower (the side panel opening). The projections, as the distance d of the point the sphere
// is projected from behind its middle (r = sin θ / (cos θ + d)): rectilinear d = 0 (straight lines straight), the
// stereographic d = 1 (shapes kept, up to nearly the whole sphere); auto goes from one to the other between 100° and
// 160°; the little planet is the stereographic looking straight down, the way ahead at the top.
//
// Filtering: Mitchell-Netravali bicubic where the view enlarges the sphere or keeps its size. Where it shrinks it (a
// wide view, near the poles, the little planet's rim) the kernel widens by as much, up to 4 times, each way on its own
// (along the longitude and the latitude), with taps half its new width apart, each a bilinear fetch, so it doesn't
// shimmer.
//
// The pass replaces MAIN, before subtitles and the looks' color shaders (the dlogm plugin's, at MAIN and later), with
// a texture the size of the output: main.lua sets keepaspect=no, so mpv shows it over the whole video area as it is.
// view360_aspect is that area's width over its height. view360_on is 0 for any clip that isn't a sphere: the pass
// doesn't run.
//
// Parts of the ideas are from mpv360 by Kacper Michajłow (https://github.com/kasper93/mpv360, MIT license): the hook,
// keepaspect off, a cubic kernel over the sphere.

//!PARAM view360_on
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 1
0

//!PARAM view360_yaw
//!TYPE DYNAMIC float
//!MINIMUM -360.0
//!MAXIMUM 360.0
0.0

//!PARAM view360_pitch
//!TYPE DYNAMIC float
//!MINIMUM -90.0
//!MAXIMUM 90.0
0.0

//!PARAM view360_roll
//!TYPE DYNAMIC float
//!MINIMUM -180.0
//!MAXIMUM 180.0
0.0

//!PARAM view360_fov
//!TYPE DYNAMIC float
//!MINIMUM 1.0
//!MAXIMUM 340.0
100.0

//!PARAM view360_projection
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 3
0

//!PARAM view360_heading
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 3
0

//!PARAM view360_stereo
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 2
0

//!PARAM view360_aspect
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 100.0
0.0

//!HOOK MAINPRESUB
//!BIND HOOKED
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!WHEN view360_on
//!DESC 360 view

// view360_projection: 0 auto, 1 rectilinear, 2 stereographic, 3 little planet
// view360_heading: 0 in the world, 1 along the camera, 2 along the way you move, 3 north (what the stamp has for each)
// view360_stereo: 0 mono, 1 top-bottom (the left eye on top), 2 left-right (the left eye at the left): the left eye shows

#define PI 3.14159265358979
#define HALF_WIDTH (16.0 / 9.0)

// ---- The stamp (step 1 of docs/360-implementation-plan.md) ----
// A LUMA pass will read the rows the plugin writes under the sphere (raw luma, texel fetches) and //!SAVE them; this
// pass will //!BIND that texture and stamp_read() decode it: the camera in the world at the middle of the frame, the
// chosen mode's heading (its yaw in the world), and how many rows of the frame are the stamp's. Until then no frame
// is stamped: a stitched clip's camera is level and fixed (the stitching software leveled it, or it was), its heading
// straight ahead, and the sphere is the whole frame.
bool stamp_read(out mat3 world_from_cam, out float heading_yaw, out float stamp_rows)
{
    world_from_cam = mat3(1.0);
    heading_yaw = 0.0;
    stamp_rows = 0.0;
    return false;
}
// ---- end of the stamp ----

// Turned about the vertical (positive: to the right), about x (positive: up), about z (positive: the picture
// counterclockwise). GLSL's mat3 takes columns
mat3 rot_yaw(float a) { float c = cos(a), s = sin(a); return mat3(c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c); }
mat3 rot_pitch(float a) { float c = cos(a), s = sin(a); return mat3(1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c); }
mat3 rot_roll(float a) { float c = cos(a), s = sin(a); return mat3(c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0); }

float projection_distance()
{
    if (view360_projection == 1) return 0.0;
    if (view360_projection >= 2) return 1.0;
    return smoothstep(100.0, 160.0, view360_fov);
}

// The ray of a point on the screen, p in half heights of the picture from its middle (y down), in the head's axes
vec3 screen_ray(vec2 p, float d, float k)
{
    float r = length(p);
    if (r < 1e-9) return vec3(0.0, 0.0, 1.0);
    float t = r / k;
    float t2 = t * t;
    float z = (-t2 * d + sqrt(t2 * (1.0 - d * d) + 1.0)) / (t2 + 1.0);
    float s = t * (z + d);
    return vec3(p / r * s, z);
}

// The sphere's texture coordinates (0-1 over the eye's part of the frame, y down) for a direction in the camera's axes
vec2 equirect(vec3 v)
{
    float lon = atan(v.x, v.z);
    float lat = asin(clamp(-v.y / length(v), -1.0, 1.0));
    return vec2(0.5 + lon / (2.0 * PI), 0.5 - lat / PI);
}

float mitchell(float x)
{
    x = abs(x);
    if (x < 1.0) return (7.0 * x * x * x - 12.0 * x * x + 16.0 / 3.0) / 6.0;
    if (x < 2.0) return (-7.0 / 3.0 * x * x * x + 12.0 * x * x - 20.0 * x + 32.0 / 3.0) / 6.0;
    return 0.0;
}

// The eye's part of the frame (origin, size), in pixels, and the frame's rows the sphere has
vec2 eye_origin;
vec2 eye_size;

vec4 fetch(vec2 t)
{
    // t in the eye's pixels (centers at whole numbers): around in longitude, held at the poles
    t.x = mod(t.x + 0.5, eye_size.x) - 0.5;
    t.y = clamp(t.y, 0.0, eye_size.y - 1.0);
    return HOOKED_tex((eye_origin + t + 0.5) / HOOKED_size);
}

// The sphere at uv, filtered over a footprint of scale.x by scale.y of its pixels
vec4 sample_sphere(vec2 uv, vec2 scale)
{
    vec2 pos = uv * eye_size - 0.5;
    vec2 step = vec2(scale.x > 2.0 ? scale.x * 0.5 : 1.0, scale.y > 2.0 ? scale.y * 0.5 : 1.0);
    // Whole pixels for an unwidened kernel (each fetch exact), the point itself otherwise
    vec2 base = vec2(step.x == 1.0 ? floor(pos.x) : pos.x, step.y == 1.0 ? floor(pos.y) : pos.y);
    ivec2 lo = ivec2(ceil((pos - 2.0 * scale - base) / step));
    ivec2 hi = ivec2(floor((pos + 2.0 * scale - base) / step));
    vec4 sum = vec4(0.0);
    float wsum = 0.0;
    for (int j = lo.y; j <= hi.y; j++) {
        float ty = base.y + float(j) * step.y;
        float wy = mitchell((ty - pos.y) / scale.y);
        if (wy == 0.0) continue;
        for (int i = lo.x; i <= hi.x; i++) {
            float tx = base.x + float(i) * step.x;
            float w = wy * mitchell((tx - pos.x) / scale.x);
            if (w == 0.0) continue;
            sum += w * fetch(vec2(tx, ty));
            wsum += w;
        }
    }
    return wsum != 0.0 ? sum / wsum : fetch(floor(pos + 0.5));
}

vec4 hook()
{
    mat3 world_from_cam;
    float heading_yaw, stamp_rows;
    stamp_read(world_from_cam, heading_yaw, stamp_rows);

    vec2 frame = vec2(HOOKED_size.x, HOOKED_size.y - stamp_rows);
    eye_origin = vec2(0.0);
    eye_size = frame;
    if (view360_stereo == 1) eye_size.y = floor(frame.y * 0.5);
    if (view360_stereo == 2) eye_size.x = floor(frame.x * 0.5);

    float aspect = view360_aspect > 0.0 ? view360_aspect : target_size.x / target_size.y;
    float d = projection_distance();
    float fov = radians(view360_fov) * 0.5;
    float k = HALF_WIDTH * (cos(fov) + d) / sin(fov);

    // The view's rotation into the world: the heading, then the head; the little planet looks down from where the head
    // is. Into the camera of this frame from there
    float pitch = view360_pitch - (view360_projection == 3 ? 90.0 : 0.0);
    mat3 head = rot_yaw(radians(view360_yaw)) * rot_pitch(radians(pitch)) * rot_roll(radians(view360_roll));
    mat3 cam_from_view = transpose(world_from_cam) * rot_yaw(heading_yaw) * head;

    // This pixel, in half heights from the middle, and the next ones right and down (for the footprint)
    vec2 p = (HOOKED_pos * 2.0 - 1.0) * vec2(aspect, 1.0);
    vec2 unit = abs(vec2(dFdx(p.x), dFdy(p.y)));
    vec2 uv = equirect(cam_from_view * screen_ray(p, d, k));
    vec2 ux = equirect(cam_from_view * screen_ray(p + vec2(unit.x, 0.0), d, k)) - uv;
    vec2 uy = equirect(cam_from_view * screen_ray(p + vec2(0.0, unit.y), d, k)) - uv;
    // Across the seam at the back, the short way round
    ux.x -= floor(ux.x + 0.5);
    uy.x -= floor(uy.x + 0.5);
    vec2 footprint = max(abs(ux), abs(uy)) * eye_size;
    return sample_sphere(uv, clamp(footprint, 1.0, 4.0));
}
