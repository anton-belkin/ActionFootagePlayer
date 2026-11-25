use super::*;

// What the options say about the picture (the job's own left out), to compare two ways of setting them. A
// video's leaves out the photo's radius; the denoise's Stronger counts only with a denoise
fn summary(o: &Options, photo: bool) -> String {
    format!("curve {:?} uw {} {} {} {:?} sdr {} nbcu {} off {} lens {:?} horizon {:?} water {:?} fov {:?} level {:?} frame {:?} edges {:?} blur {} \
             denoise {} strong {} chromatic {} interp {:?} gpu_interp {:?} translation {} {} preset {:?} project {:?} res {:?} quality {} \
             dv {:?} {} sdr_image {:?} grain {} sphere {:?} radius {}",
            o.curve, o.uw, o.uw_settings.strength, o.uw_settings.highlights, o.uw_params, o.sdr, o.nbcu, o.no_stabilize, o.lens, o.horizon_lock,
            o.underwater, o.fov, o.level, o.frame, o.edges, o.edge_settings.blur, o.denoise, o.denoise_strong && o.denoise > 0, o.chromatic,
            o.interpolation as i32, o.gpu_interp, o.with_translation, o.translation_kept, o.preset, o.project, o.resolution, o.encode_quality,
            o.dv_rpu, o.dv_live_l1, o.sdr_image, o.photo_grain, o.sphere, if photo { o.photo_radius as i64 } else { -1 })
}

fn with_settings(j: serde_json::Value) -> Options {
    let mut o = default_options();
    apply_settings(&mut o, &j).unwrap_or_else(|e| panic!("{j}: {e}"));
    o
}

fn repo(file: &str) -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(file) }

// tests/settings_cases.json: a saved view's settings set the options its flags did (the player's view_args before)
#[test]
fn settings_set_what_their_flags_did() {
    let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(repo("tests/settings_cases.json")).unwrap()).unwrap();
    let cases = cases["cases"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let photo = c["kind"] == "photo";
        let flags: Vec<String> = c["flags"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
        let (by_flags, _) = parse_args(flags.into_iter());
        assert_eq!(summary(&with_settings(c["settings"].clone()), photo), summary(&by_flags, photo), "{name}");
    }
}

// Every key registry.json says gyroflow-export reads is read (an unread one is an error), and its example changes
// the options; the table's and the request's keys are left alone
#[test]
fn every_key_the_export_takes_is_read() {
    let dir = std::env::temp_dir().join(format!("afp-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let preset = dir.join("default.gyroflow");
    std::fs::write(&preset, "{}").unwrap();
    let raw = |o: &Options| format!("{} {}", summary(o, true), o.denoise_strong);
    let none = raw(&default_options());
    let reg = vs_gyroflow::lists::get();
    assert!(reg.settings_json.len() > 30);
    for k in &reg.settings_json {
        let mut v = k.example.clone().unwrap();
        if k.key == "preset" { v = serde_json::json!(preset.to_string_lossy()); }
        let mut o = default_options();
        let r = apply_settings(&mut o, &serde_json::json!({ k.key.as_str(): v }));
        if k.export {
            r.unwrap_or_else(|e| panic!("{}: {e}", k.key));
            assert_ne!(raw(&o), none, "{}: its example {v} changes nothing", k.key);
        } else {
            assert!(r.is_ok() && raw(&o) == none, "{}: not gyroflow-export's, yet it read it", k.key);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    // A key it doesn't know is left (the table's, another program's); a wrong value for one it reads isn't
    assert!(apply_settings(&mut default_options(), &serde_json::json!({ "a_key_of_the_future": 1 })).is_ok());
    assert!(apply_settings(&mut default_options(), &serde_json::json!({ "level": "extreme" })).is_err());
}

// #313: Ctrl+j's translation in the settings is taken where it's computed; #314: the grain; #320: the sphere's width,
// its stitching distance and heading reach a 360 export; #319: a photo's clean-up follows the settings' denoise
#[test]
fn settings_the_flags_left_out() {
    assert!(with_settings(serde_json::json!({ "translation": true })).translation_kept, "#313");
    assert_eq!(with_settings(serde_json::json!({ "denoise_grain": 0.6 })).photo_grain, 0.6, "#314");
    let (o, files) = parse_args(["--settings", r#"{"sphere_width": 4096, "stitching_distance": 3.5, "heading": "north"}"#, "--360", "a.insv"].map(String::from).into_iter());
    let m = o.sphere360.as_ref().unwrap();
    assert_eq!((m.sphere_width, m.distance, m.heading), (4096, 3.5, Some(vs_gyroflow::stamp::Heading::North)), "#320");
    assert_eq!(files, ["a.insv"]);
    // The command line's own width wins
    let (o, _) = parse_args(["--settings", r#"{"sphere_width": 4096}"#, "--sphere-width", "2048", "--360", "a.insv"].map(String::from).into_iter());
    assert_eq!(o.sphere360.unwrap().sphere_width, 2048);
    // #319: the settings' denoise decides, whatever this computer plays
    assert_eq!(with_settings(serde_json::json!({ "denoise": true, "temporal_denoise": "light" })).photo_radius, 5);
    assert_eq!(with_settings(serde_json::json!({ "denoise": true, "temporal_denoise": "off" })).photo_radius, 0);
    assert_eq!(with_settings(serde_json::json!({ "denoise": false, "still_radius": 7 })).photo_radius, 7);
    // The player's request as-is: the video itself as the project is no project; a .gyroflow is
    assert!(with_settings(serde_json::json!({ "project": "/clips/GX010001.MP4" })).project.is_none());
    assert!(with_settings(serde_json::json!({ "project": "/clips/GX010001.gyroflow" })).project.is_some());
    // Flags after --settings change what it set
    let (o, _) = parse_args(["--settings", r#"{"level": "weak", "edges": "smooth"}"#, "--level", "strong", "v.mp4"].map(String::from).into_iter());
    assert_eq!((o.level, o.edges), (Level::Strong, EdgeMode::Smooth));
}

// GOPR1585: a clip with no motion data of its own and its motion measured from the picture beside it is exported
// stabilized with that, not with the lens alone
#[test]
fn measured_motion_is_exported_stabilized() {
    let dir = std::env::temp_dir().join(format!("afp-export-measured-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let video = dir.join("GOPR0001.MP4");
    std::fs::write(&video, vec![0u8; 4321]).unwrap();
    // Nothing to stabilize it with, no lens: not exported
    assert!(find_source(&video, None, vs_gyroflow::LensMode::Stabilized, None).is_err());
    let header = vs_gyroflow::afp_motion::Header { clip: vs_gyroflow::afp_motion::ClipId::of(&video).unwrap(), ..Default::default() };
    let correction = gyroflow_core::gyro_source::OpticalCorrection { enabled: true, frames: 2, measured_frames: 1,
        video_base: vec![(0, [1.0, 0.0, 0.0, 0.0]), (33_366, [1.0, 0.0, 0.0, 0.0])], ..Default::default() };
    vs_gyroflow::afp_motion::MotionFile::new(header, correction).unwrap().write(&vs_gyroflow::afp_motion::path_for(&video)).unwrap();
    let source = find_source(&video, None, vs_gyroflow::LensMode::Stabilized, None).expect("stabilized with its measured motion");
    assert_eq!(source.label, "motion measured from the picture");
    assert!(source.lens != vs_gyroflow::LensMode::Only);
    let _ = std::fs::remove_dir_all(&dir);
}

// docs/design/export-settings.md: the export's settings (--resolution, --encode, the quality preset's --interpolation) on
// each path's command line
#[test]
fn export_settings_on_the_command_line() {
    // --resolution: the short side at most, the shape kept, even, never larger; original: as it is
    assert_eq!(parse_resolution("original"), Some(None));
    assert_eq!(parse_resolution("1080"), Some(Some(1080)));
    assert_eq!(parse_resolution("1080p"), Some(Some(1080)));
    assert_eq!(parse_resolution("4K"), Some(Some(2160)));
    assert_eq!(parse_resolution("huge"), None);
    assert_eq!(capped((3840, 2160), Some(1080)), (1920, 1080));
    assert_eq!(capped((3840, 2160), Some(720)), (1280, 720));
    assert_eq!(capped((2160, 3840), Some(1080)), (1080, 1920), "portrait: its width is the short side");
    assert_eq!(capped((4000, 3000), Some(1080)), (1440, 1080), "4:3");
    assert_eq!(capped((2704, 1520), Some(1080)), (1920, 1080), "2.7K: 1921.3 wide, even");
    assert_eq!(capped((1920, 1080), Some(2160)), (1920, 1080), "never enlarged");
    assert_eq!(capped((3840, 2160), None), (3840, 2160));
    // The bit rate: the source's for the size, quality half as much again
    assert_eq!(export_bit_rate(Some(120_000_000), 3840 * 2160, 1920 * 1080, false), 30_000_000.0);
    assert_eq!(export_bit_rate(Some(120_000_000), 3840 * 2160, 3840 * 2160, true), 180_000_000.0);
    // --encode: the GPU path's VideoToolbox real-time by default (as before), the pipes the encoder's default (as
    // before); quality its quality mode on both
    let s = |v: Vec<String>| v.join(" ");
    assert_eq!(s(encode_mode_args("hevc_videotoolbox", false, true)), "-realtime 1 -prio_speed 1");
    assert_eq!(s(encode_mode_args("hevc_videotoolbox", false, false)), "");
    assert_eq!(s(encode_mode_args("hevc_videotoolbox", true, true)), "-realtime 0 -prio_speed 0");
    assert_eq!(s(encode_mode_args("hevc_videotoolbox", true, false)), "-realtime 0 -prio_speed 0");
    assert_eq!(s(encode_mode_args("hevc_nvenc", true, false)), "-preset p7 -tune hq");
    assert_eq!(s(encode_mode_args("libx265", true, true)), "-preset slow");
    assert_eq!(s(encode_mode_args("hevc_vulkan", true, true)), "", "Vulkan's encoder has no mode: the bit rate alone");
    // The pipes scale by --interpolation's filter (quickest: bilinear; highest and the default: Lanczos); not at all
    // at the same size. The GPU path has no scale: the warp renders at the size (libplacebo's w and h)
    assert_eq!(scale_filter((3840, 2160), (1920, 1080), Interpolation::Lanczos4), "scale=1920:1080:flags=lanczos");
    assert_eq!(scale_filter((3840, 2160), (1920, 1080), Interpolation::Bilinear), "scale=1920:1080:flags=bilinear");
    assert_eq!(scale_filter((3840, 2160), (3840, 2160), Interpolation::Bilinear), "");
}

#[test]
fn dates() {
    let d = parse_dates("2026-10-22T15:28:42+02:00").unwrap();
    assert_eq!(d.offset_min, 120);
    assert_eq!(d.utc(), "2026-10-22T13:28:42.000000Z");
    assert_eq!(d.quicktime(), "2026-10-22T15:28:42+0200");
    assert_eq!(d.system_time().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), 1792675722);
    // Across midnight and the year, west of UTC, compact zone, fractions
    let d = parse_dates("2027-01-01T01:30:00.250-0500").unwrap();
    assert_eq!(d.utc(), "2027-01-01T06:30:00.250000Z");
    assert_eq!(d.quicktime(), "2027-01-01T01:30:00-0500");
    let d = parse_dates("2026-12-31T23:30:00+01:00").unwrap();
    assert_eq!(d.utc(), "2026-12-31T22:30:00.000000Z");
    assert_eq!(parse_dates("2024-02-29T12:00:00Z").unwrap().quicktime(), "2024-02-29T12:00:00+0000");
    for bad in ["2026-10-22", "2026-10-22T15:28:42", "2026-13-01T00:00:00Z", "2026-10-22T25:00:00Z", "yesterday", "2026-10-22T15:28:42+2"] {
        assert!(parse_dates(bad).is_none(), "{bad}");
    }
}

#[test]
fn zones() {
    let created = parse_dates("2026-10-24T06:45:41Z").unwrap().unix;
    assert_eq!(zone_of_name("DJI_20261024094541_0093_D", created), Some(180));
    assert_eq!(zone_of_name("GX011057", created), None);
    assert_eq!(zone_of_name("DJI_2026", created), None);
}

#[test]
fn flipbook_frames() {
    assert_eq!(flipbook_times(43.8).len(), 4);
    assert!(flipbook_times(43.8).windows(2).all(|w| w[1] - w[0] >= 10.0));
    assert!(flipbook_times(43.8)[0] > 1.0);
    assert_eq!(flipbook_times(600.0).len(), 12);
    assert_eq!(flipbook_times(5.0).len(), 1);
    assert_eq!(flipbook_times(1.0), vec![0.5]);
}

#[test]
fn preview_sizes() {
    assert_eq!(preview_size(3840, 2160), (960, 540));
    assert_eq!(preview_size(2160, 3840), (540, 960));
    assert_eq!(preview_size(4000, 3000), (960, 720));
    assert_eq!(preview_size(640, 360), (640, 360));
    assert_eq!(preview_size(2688, 1512), (960, 540));
}
