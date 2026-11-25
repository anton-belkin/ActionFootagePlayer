// The player's own shaders over the engine's frames: the water's colour fringes, the temporal denoise.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// ---------------------------------------------------------------------------------------------------
// What the player draws over the stabilized frame, by its own shaders (vs_gyroflow::user_shader): the water's color
// fringes taken out, as chromatic.glsl does it in mpv from the marker the engine writes into each frame, and the
// temporal denoise, the GPU path's (warp.glsl's dn_ passes), each frame from the frames around it, which an export
// has on both sides. In the player's order: the fringes, then the denoise (on the GPU path both follow the warp)

const CHROMATIC_GLSL: &str = include_str!("../../../../mpv/scripts/gyroflow/chromatic.glsl");

pub(crate) struct Post {
    pub(crate) shaders: vs_gyroflow::user_shader::Shaders,
    /// The engine writes each frame's marker, for chromatic.glsl
    pub(crate) chromatic: bool,
    /// The denoise's frames on each side (0: none): the frame it shows is that many behind the newest one it has, as in
    /// the player (gyroflow_dn_after)
    pub(crate) radius: usize,
    fps: f64,
    pub(crate) label: String,
}

impl Post {
    /// For the frames `range` (all the export decodes) rendered at `size`, as the engine renders them (its view at the
    /// level, the camera); `denoise`: the frames on each side, 0 none. None when there's nothing to do: no denoise, and
    /// no frame of it with the lens under water (or the fringes left as recorded)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(engine: &Engine, opts: &Options, info: &VideoInfo, size: (usize, usize), fps: f64, range: (u64, u64), view: u8, camera: vs_gyroflow::Camera,
           denoise: usize) -> Option<Post> {
        use vs_gyroflow::gpu_table::{ self, Clip, Mode, Table };
        let index = engine.views.index(view, opts.level).min(engine.views.params.len() - 1);
        let params = camera.apply(&engine.views.params[index]);
        let clip = Clip { size: (params.width, params.height), output_size: (params.output_width, params.output_height), fps,
                          frame_count: params.frame_count.max(1), frame_offset_us: engine.frame_offset_us, params };
        let (first, last) = ((range.0 as usize).min(clip.frame_count - 1), (range.1 as usize).min(clip.frame_count - 1));
        // The fringes where some frame of it has the lens under water and fully corrected, as the player's GPU path
        // tells (gyroflow-gpu-table); chromatic.glsl leaves the others as they are
        let chromatic = opts.chromatic && (first..=last).step_by(15).chain(std::iter::once(last)).any(|f| {
            let kp = clip.transform(f, clip.output_size).kernel_params;
            kp.light_refraction_coefficient > 1.0 && kp.lens_correction_amount >= 0.999
        });
        let mut sources = Vec::new();
        let mut labels = Vec::new();
        if chromatic {
            // AFP_CHROMATIC_GLSL: another chromatic.glsl, for work on the shader without a rebuild
            sources.push(std::env::var_os("AFP_CHROMATIC_GLSL").and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| CHROMATIC_GLSL.to_string()));
            labels.push("color fringes taken out".to_string());
        }
        let mut data = Vec::new();
        let mut params = Vec::new();
        let mut radius = 0;
        if denoise > 0 {
            // The table the denoise predicts the motion between frames by (each frame's camera; two knots a frame
            // are enough, the rows' rotations aren't read)
            let o = gpu_table::Options { mode: Mode::Exact, grid_step: 8, grid_linear: false, chroma_bilinear: false, chromatic: false, chromatic_fast: false,
                                         edge_fill: None, sphere: false, time_offset: 0.0, denoise: true, knots: 2, interp: 0, shrink: false, range: Some((first, last)) };
            match Table::build(&clip, &o).map(|t| (gpu_table::denoise_shader(&clip, &o, &t, size), t)) {
                Ok((Some(text), table)) => {
                    sources.push(text);
                    data.push(("GYROFLOW_TABLE".to_string(), table.data));
                    params = vec![("gyroflow_dn_radius".to_string(), denoise as f64), ("gyroflow_dn_after".to_string(), denoise as f64),
                                  ("gyroflow_dn_strong".to_string(), opts.denoise_strong as i32 as f64), ("gyroflow_dn_keep".to_string(), opts.photo_grain as f64),
                                  ("gyroflow_gpu_on".to_string(), 1.0), ("gyroflow_render_scale".to_string(), 1.0)];
                    radius = denoise;
                    labels.push(format!("temporal denoise {}{}", if denoise >= 5 { "full" } else { "light" }, if opts.denoise_strong { ", stronger" } else { "" }));
                }
                Ok((None, _)) => eprintln!("  no temporal denoise: {}x{} is more than it takes", size.0, size.1),
                Err(e) => eprintln!("  no temporal denoise: {e}"),
            }
        }
        if sources.is_empty() { return None; }
        let matrix = gpu_lut::matrix_coefficients(yuv_matrix(info.matrix.as_deref())).unwrap_or((0.2126, 0.0722));
        let config = |sources: Vec<String>, data, params| vs_gyroflow::user_shader::Config { sources, size, prelude: format!("#define RT vec2({}.0, {}.0)", size.0, size.1),
                                                                                        data, matrix, full_range: info.full_range, params };
        match vs_gyroflow::user_shader::Shaders::new(config(sources.clone(), data, params)) {
            Ok(shaders) => Some(Post { shaders, chromatic, radius, fps, label: labels.join(", ") }),
            Err(e) => {
                eprintln!("  not as the player draws it ({}): {e}", labels.join(", "));
                // The fringes alone, without the denoise that failed
                if !(chromatic && radius > 0) { return None; }
                let shaders = vs_gyroflow::user_shader::Shaders::new(config(sources[..1].to_vec(), Vec::new(), Vec::new())).ok()?;
                Some(Post { shaders, chromatic, radius: 0, fps, label: labels[0].clone() })
            }
        }
    }

    /// The frames in order (`first`: the first one's index in the clip), each through the shaders; `emit` gets each
    /// frame that comes out (its index in the clip, the frame). With the denoise a frame comes out when the frames
    /// after it are in, the last ones at the end with the frames there are after them (as the player shows a clip's
    /// last frames)
    pub(crate) fn run(mut self, first: u64, frames: mpsc::Receiver<Vec<u8>>, emit: &mut dyn FnMut(u64, Vec<u8>) -> Result<(), String>) -> Result<(), String> {
        // The frames before the newest the denoise's window counts (warp.glsl's 2 x DN_RADIUS)
        const BACK: u64 = 10;
        let after = self.radius as u64;
        let mut fed = 0u64;
        let mut last: Option<(u64, Vec<u8>)> = None;
        let mut gpu = 0.0;
        for buf in frames {
            let f = first + fed;
            self.shaders.set("PTS", f as f64 / self.fps);
            let out = self.shaders.render(&buf)?;
            gpu += self.shaders.millis;
            let shown = after.min(fed.min(BACK));
            fed += 1;
            if shown == after { emit(f - after, out)?; }
            last = Some((f, buf));
        }
        if let Some((f, buf)) = last {
            for k in (0..after.min(fed)).rev() {
                self.shaders.set("gyroflow_dn_after", k as f64);
                let out = self.shaders.render(&buf)?;
                emit(f - k, out)?;
            }
        }
        if fed > 0 && std::env::var_os("AFP_TIMES").is_some() { eprintln!("  the player's shaders: {:.1} ms a frame", gpu / fed as f64); }
        Ok(())
    }
}
