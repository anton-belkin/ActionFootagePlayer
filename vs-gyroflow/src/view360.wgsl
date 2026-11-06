// The 360 view in WGSL, for gyroflow-export (view360.rs): the same math as the player's mpv shader,
// mpv/scripts/view360/view360.glsl ("360 view" pass), function for function, so that an export looks as the player
// shows it. A change to one is a change to the other: view360.rs's tests render both from the same sphere and stamp
// (the GLSL through naga's GLSL front end) and compare.
//
// What differs, and only in what the player has no use for:
// - the stamp comes in as parameters (the export reads it in Rust, stamp.rs), not through the LUMA pass;
// - projection 4, the equirect: the whole sphere out, turned into the world and the heading (a 360 export);
// - the output's own aspect (the player's is the window's video area).
// Conventions are video-types' (x right, y down, z forward; the equirect's longitude 0 at +z, +90° at +x, latitude +90°
// up), as there.

struct Params {
    // The camera at the middle of the frame in the world, Hamilton w x y z (the stamp's q_world_from_cam)
    q: vec4<f32>,
    // The texture's size in pixels (the sphere, and the stamp's rows if they're in it), and the output's
    size: vec2<f32>,
    out_size: vec2<f32>,
    // The chosen heading's yaw in the world, radians (the stamp's)
    heading_yaw: f32,
    // The head, degrees, and the FOV across a 16:9 picture of the output's height
    yaw: f32,
    pitch: f32,
    roll: f32,
    fov: f32,
    // 0 auto, 1 rectilinear, 2 stereographic, 3 little planet, 4 equirect (the whole sphere)
    projection: i32,
    // 0 mono, 1 top-bottom, 2 left-right: the left eye
    stereo: i32,
    // The texture's rows under the sphere that aren't sphere (the stamp's)
    stamp_rows: f32,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var sphere: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

const PI: f32 = 3.14159265358979;
const HALF_WIDTH: f32 = 16.0 / 9.0;

// GLSL's mod: x - y·floor(x/y) (WGSL's % truncates)
fn glsl_mod(x: f32, y: f32) -> f32 { return x - y * floor(x / y); }

// The stamp's quaternion as a matrix, columns (stamp_read)
fn world_from_cam_of(q4: vec4<f32>) -> mat3x3<f32> {
    let q = q4 / length(q4);
    let w = q.x; let x = q.y; let y = q.z; let z = q.w;
    return mat3x3<f32>(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + w * z), 2.0 * (x * z - w * y),
                       2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x),
                       2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y));
}

fn rot_yaw(a: f32) -> mat3x3<f32> { let c = cos(a); let s = sin(a); return mat3x3<f32>(c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c); }
fn rot_pitch(a: f32) -> mat3x3<f32> { let c = cos(a); let s = sin(a); return mat3x3<f32>(1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c); }
fn rot_roll(a: f32) -> mat3x3<f32> { let c = cos(a); let s = sin(a); return mat3x3<f32>(c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0); }

fn projection_distance() -> f32 {
    if (u.projection == 1) { return 0.0; }
    if (u.projection >= 2) { return 1.0; }
    return smoothstep(100.0, 160.0, u.fov);
}

fn screen_ray(p: vec2<f32>, d: f32, k: f32) -> vec3<f32> {
    let r = length(p);
    if (r < 1e-9) { return vec3<f32>(0.0, 0.0, 1.0); }
    let t = r / k;
    let t2 = t * t;
    let z = (-t2 * d + sqrt(t2 * (1.0 - d * d) + 1.0)) / (t2 + 1.0);
    let s = t * (z + d);
    return vec3<f32>(p / r * s, z);
}

// The direction of an output pixel of the equirect (projection 4), pos 0-1 over the output, y down
fn equirect_ray(pos: vec2<f32>) -> vec3<f32> {
    let lon = (pos.x - 0.5) * 2.0 * PI;
    let lat = (0.5 - pos.y) * PI;
    return vec3<f32>(cos(lat) * sin(lon), -sin(lat), cos(lat) * cos(lon));
}

fn equirect(v: vec3<f32>) -> vec2<f32> {
    let lon = atan2(v.x, v.z);
    let lat = asin(clamp(-v.y / length(v), -1.0, 1.0));
    return vec2<f32>(0.5 + lon / (2.0 * PI), 0.5 - lat / PI);
}

fn mitchell(x_in: f32) -> f32 {
    let x = abs(x_in);
    if (x < 1.0) { return (7.0 * x * x * x - 12.0 * x * x + 16.0 / 3.0) / 6.0; }
    if (x < 2.0) { return (-7.0 / 3.0 * x * x * x + 12.0 * x * x - 20.0 * x + 32.0 / 3.0) / 6.0; }
    return 0.0;
}

var<private> eye_origin: vec2<f32>;
var<private> eye_size: vec2<f32>;

fn fetch(t_in: vec2<f32>) -> vec4<f32> {
    var t = t_in;
    t.x = glsl_mod(t.x + 0.5, eye_size.x) - 0.5;
    t.y = clamp(t.y, 0.0, eye_size.y - 1.0);
    return textureSampleLevel(sphere, samp, (eye_origin + t + 0.5) / u.size, 0.0);
}

fn sample_sphere(uv: vec2<f32>, scale: vec2<f32>) -> vec4<f32> {
    let pos = uv * eye_size - 0.5;
    let step = vec2<f32>(select(1.0, scale.x * 0.5, scale.x > 2.0), select(1.0, scale.y * 0.5, scale.y > 2.0));
    let base = vec2<f32>(select(pos.x, floor(pos.x), step.x == 1.0), select(pos.y, floor(pos.y), step.y == 1.0));
    let lo = vec2<i32>(ceil((pos - 2.0 * scale - base) / step));
    let hi = vec2<i32>(floor((pos + 2.0 * scale - base) / step));
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var j = lo.y; j <= hi.y; j++) {
        let ty = base.y + f32(j) * step.y;
        let wy = mitchell((ty - pos.y) / scale.y);
        if (wy == 0.0) { continue; }
        for (var i = lo.x; i <= hi.x; i++) {
            let tx = base.x + f32(i) * step.x;
            let w = wy * mitchell((tx - pos.x) / scale.x);
            if (w == 0.0) { continue; }
            sum += w * fetch(vec2<f32>(tx, ty));
            wsum += w;
        }
    }
    if (wsum != 0.0) { return sum / wsum; }
    return fetch(floor(pos + 0.5));
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let world_from_cam = world_from_cam_of(u.q);
    let frame = vec2<f32>(u.size.x, u.size.y - u.stamp_rows);
    eye_origin = vec2<f32>(0.0);
    eye_size = frame;
    if (u.stereo == 1) { eye_size.y = floor(frame.y * 0.5); }
    if (u.stereo == 2) { eye_size.x = floor(frame.x * 0.5); }

    let pos = frag.xy / u.out_size;
    let aspect = u.out_size.x / u.out_size.y;
    let d = projection_distance();
    let fov = radians(u.fov) * 0.5;
    let k = HALF_WIDTH * (cos(fov) + d) / sin(fov);

    var pitch = u.pitch;
    if (u.projection == 3) { pitch -= 90.0; }
    let head = rot_yaw(radians(u.yaw)) * rot_pitch(radians(pitch)) * rot_roll(radians(u.roll));
    let cam_from_view = transpose(world_from_cam) * rot_yaw(u.heading_yaw) * head;

    var uv: vec2<f32>;
    var ux: vec2<f32>;
    var uy: vec2<f32>;
    if (u.projection == 4) {
        // The equirect: its pixel's direction, and its neighbours' right and down
        let unit = 1.0 / u.out_size;
        uv = equirect(cam_from_view * equirect_ray(pos));
        ux = equirect(cam_from_view * equirect_ray(pos + vec2<f32>(unit.x, 0.0))) - uv;
        uy = equirect(cam_from_view * equirect_ray(pos + vec2<f32>(0.0, unit.y))) - uv;
    } else {
        let p = (pos * 2.0 - 1.0) * vec2<f32>(aspect, 1.0);
        let unit = abs(vec2<f32>(dpdx(p.x), dpdy(p.y)));
        uv = equirect(cam_from_view * screen_ray(p, d, k));
        ux = equirect(cam_from_view * screen_ray(p + vec2<f32>(unit.x, 0.0), d, k)) - uv;
        uy = equirect(cam_from_view * screen_ray(p + vec2<f32>(0.0, unit.y), d, k)) - uv;
    }
    ux.x -= floor(ux.x + 0.5);
    uy.x -= floor(uy.x + 0.5);
    let footprint = max(abs(ux), abs(uy)) * eye_size;
    return sample_sphere(uv, clamp(footprint, vec2<f32>(1.0), vec2<f32>(4.0)));
}
