// Spike 1 (docs/mobile-app.md): our mpv (libmpv, vo=gpu-next on MoltenVK, VideoToolbox) playing a clip with the
// Mac player's own shaders: the warp as gyroflow-gpu-table --player writes it (here in the app's process), and the
// hdr look (dlogm_hlg.glsl, the video tagged HLG). On screen: mpv's frame rate, drops and the warp's GPU time.
import SwiftUI
import QuartzCore
import AVFoundation

final class MpvLayerView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }
    var metalLayer: CAMetalLayer { layer as! CAMetalLayer }
    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        metalLayer.wantsExtendedDynamicRangeContent = true
    }
    required init?(coder: NSCoder) { fatalError() }
    override func layoutSubviews() {
        super.layoutSubviews()
        let s = window?.screen.nativeScale ?? UIScreen.main.nativeScale
        metalLayer.contentsScale = s
        metalLayer.drawableSize = CGSize(width: bounds.width * s, height: bounds.height * s)
    }
}

@MainActor
final class MpvModel: ObservableObject {
    let item: PlayItem
    let view = MpvLayerView()
    let curve: LogCurve
    private var mpv: OpaquePointer?
    private var warpShader: String?
    @Published var status = "Starting mpv…"
    @Published var stats = ""
    @Published var playing = false
    @Published var time: Double = 0
    @Published var length: Double = 1
    @Published var warpReady = false
    @Published var stabilize = true { didSet { setOpt("gyroflow_gpu_on", stabilize ? "1" : "0") } }
    private var statsTimer: Timer?
    private var startDrops: Int64 = -1

    init(item: PlayItem, interpolation: String) {
        self.item = item
        curve = LogCurve.guess(item.url)
        start(interpolation: interpolation)
    }

    private func opt(_ k: String, _ v: String) { _ = mpv_set_option_string(mpv, k, v) }

    private func start(interpolation: String) {
        guard let h = mpv_create() else { status = "mpv_create failed"; return }
        mpv = h
        opt("config", "no")
        opt("terminal", "no")
        opt("msg-level", "all=warn")
        opt("vo", "gpu-next")
        opt("gpu-api", "vulkan")
        opt("gpu-context", "iosvk")
        opt("hwdec", "videotoolbox")
        opt("keep-open", "yes")
        opt("target-colorspace-hint", "yes")
        // The iOS context knows no refresh rate: the screen's (60, or 120 on ProMotion)
        opt("display-fps-override", String(UIScreen.main.maximumFramesPerSecond))
        var wid = Int64(Int(bitPattern: Unmanaged.passUnretained(view.metalLayer).toOpaque()))
        mpv_set_option(h, "wid", MPV_FORMAT_INT64, &wid)
        if item.start > 0 { opt("start", String(item.start)) }
        if let d = item.duration {
            opt("ab-loop-a", String(item.start))
            opt("ab-loop-b", String(item.start + d))
        }
        // The hdr look, as the dlogm plugin sets it: the LUT's HLG passed through unchanged
        if curve != .none, let lut = Bundle.main.path(forResource: curve.rawValue, ofType: "glsl") {
            opt("vf", "format:gamma=hlg:primaries=bt.2020")
            opt("target-trc", "hlg")
            opt("target-prim", "bt.2020")
            opt("target-peak", "1000")
            opt("glsl-shaders", lut)
        }
        guard mpv_initialize(h) >= 0 else { status = "mpv_initialize failed"; return }
        mpv_request_log_messages(h, "info")
        mpv_observe_property(h, 1, "time-pos", MPV_FORMAT_DOUBLE)
        mpv_observe_property(h, 2, "duration", MPV_FORMAT_DOUBLE)
        mpv_observe_property(h, 3, "pause", MPV_FORMAT_FLAG)
        mpv_set_wakeup_callback(h, { ctx in
            guard let ctx else { return }
            let m = Unmanaged<MpvModel>.fromOpaque(ctx).takeUnretainedValue()
            DispatchQueue.main.async { m.drain() }
        }, Unmanaged.passUnretained(self).toOpaque())
        command(["loadfile", item.url.path])
        status = "Reading motion data…"
        statsTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.updateStats() }
        }
        Task { await buildWarp(interpolation: interpolation) }
    }

    func command(_ args: [String]) {
        guard let mpv else { return }
        var cargs = args.map { strdup($0) } + [nil]
        defer { cargs.forEach { free($0) } }
        cargs.withUnsafeMutableBufferPointer { p in
            p.baseAddress!.withMemoryRebound(to: UnsafePointer<CChar>?.self, capacity: p.count) { _ = mpv_command(mpv, $0) }
        }
    }

    private func setOpt(_ name: String, _ value: String) { command(["change-list", "glsl-shader-opts", "add", "\(name)=\(value)"]) }

    func buildWarp(interpolation: String) async {
        let base = item.url.deletingPathExtension()
        let gyroflow = base.appendingPathExtension("gyroflow")
        let project = FileManager.default.fileExists(atPath: gyroflow.path) ? gyroflow : item.url
        let asset = AVURLAsset(url: item.url)
        guard let track = try? await asset.loadTracks(withMediaType: .video).first,
              let size = try? await track.load(.naturalSize) else { status = "Can't read this video"; return }
        let json: [String: Any] = ["project": project.path, "input_width": Int(size.width), "input_height": Int(size.height),
                                   "interpolation": interpolation]
        let text = String(data: try! JSONSerialization.data(withJSONObject: json), encoding: .utf8)!
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("warp-\(UUID().uuidString).glsl").path
        let t0 = Date()
        let reply: [String: Any] = await Task.detached(priority: .userInitiated) {
            guard let r = afp_mpv_shader(text, out) else { return ["error": "no reply"] }
            defer { afp_string_free(r) }
            let s = String(cString: r)
            return (try? JSONSerialization.jsonObject(with: Data(s.utf8)) as? [String: Any]) ?? ["error": s]
        }.value
        if let e = reply["error"] as? String { status = "Not stabilized: \(e)"; return }
        if let u = reply["unsupported"] as? String { status = "Not stabilized on the GPU: \(u)"; return }
        // The warp first, at MAIN, then the look
        command(["change-list", "glsl-shaders", "pre", out])
        warpShader = out
        warpReady = true
        status = String(format: "mpv: stabilized here, %@ (motion %.1f s)", reply["mode"] as? String ?? "", Date().timeIntervalSince(t0))
    }

    private func drain() {
        guard let mpv else { return }
        while true {
            guard let ev = mpv_wait_event(mpv, 0) else { break }
            let e = ev.pointee
            if e.event_id == MPV_EVENT_NONE { break }
            if e.event_id == MPV_EVENT_PROPERTY_CHANGE, let data = e.data {
                let p = data.assumingMemoryBound(to: mpv_event_property.self).pointee
                let name = String(cString: p.name)
                guard let v = p.data else { continue }
                switch name {
                case "time-pos": time = v.assumingMemoryBound(to: Double.self).pointee - item.start
                case "duration": length = item.duration ?? max(v.assumingMemoryBound(to: Double.self).pointee - item.start, 0.1)
                case "pause": playing = v.assumingMemoryBound(to: Int32.self).pointee == 0
                default: break
                }
            } else if e.event_id == MPV_EVENT_LOG_MESSAGE, let data = e.data {
                let m = data.assumingMemoryBound(to: mpv_event_log_message.self).pointee
                NSLog("mpv[%@] %@", String(cString: m.prefix), String(cString: m.text).trimmingCharacters(in: .newlines))
            } else if e.event_id == MPV_EVENT_END_FILE {
                let ef = e.data?.assumingMemoryBound(to: mpv_event_end_file.self).pointee
                if let ef, ef.reason == MPV_END_FILE_REASON_ERROR { status = "mpv: \(String(cString: mpv_error_string(ef.error)))" }
            }
        }
    }

    private func getDouble(_ name: String) -> Double? {
        var v = 0.0
        return mpv_get_property(mpv, name, MPV_FORMAT_DOUBLE, &v) >= 0 ? v : nil
    }
    private func getInt(_ name: String) -> Int64? {
        var v: Int64 = 0
        return mpv_get_property(mpv, name, MPV_FORMAT_INT64, &v) >= 0 ? v : nil
    }

    /// The warp's and the whole frame's GPU time (vo-passes: libplacebo's per pass timings, the fresh frames')
    private func passTimes() -> (warp: Double, all: Double)? {
        var node = mpv_node()
        guard mpv_get_property(mpv, "vo-passes", MPV_FORMAT_NODE, &node) >= 0 else { return nil }
        defer { mpv_free_node_contents(&node) }
        func map(_ n: mpv_node) -> [String: mpv_node] {
            guard n.format == MPV_FORMAT_NODE_MAP, let l = n.u.list?.pointee else { return [:] }
            var d: [String: mpv_node] = [:]
            for i in 0..<Int(l.num) { d[String(cString: l.keys[i]!)] = l.values[i] }
            return d
        }
        func list(_ n: mpv_node) -> [mpv_node] {
            guard n.format == MPV_FORMAT_NODE_ARRAY, let l = n.u.list?.pointee else { return [] }
            return (0..<Int(l.num)).map { l.values[$0] }
        }
        guard let fresh = map(node)["fresh"] else { return nil }
        var warp = 0.0, all = 0.0
        for pass in list(fresh) {
            let m = map(pass)
            let desc = m["desc"].flatMap { $0.format == MPV_FORMAT_STRING ? String(cString: $0.u.string) : nil } ?? ""
            let avg = m["avg"].map { Double($0.u.int64) / 1e6 } ?? 0
            all += avg
            if desc.lowercased().contains("gyroflow") { warp += avg }
        }
        return (warp, all)
    }

    private func updateStats() {
        guard mpv != nil else { return }
        let fps = getDouble("estimated-vf-fps") ?? 0
        let disp = getDouble("display-fps") ?? 0
        let drops = getInt("frame-drop-count") ?? 0
        let late = getInt("vo-delayed-frame-count") ?? 0
        let hw = mpv_get_property_string(mpv, "hwdec-current").map { s -> String in defer { mpv_free(s) }; return String(cString: s) } ?? "?"
        var t = String(format: "%.1f fps (screen %.0f) · dropped %lld · late %lld · hwdec %@", fps, disp, drops, late, hw)
        if let p = passTimes() { t += String(format: " · GPU %.2f ms (warp %.2f)", p.all, p.warp) }
        stats = t
    }

    func togglePlay() { command(["cycle", "pause"]) }
    func seek(_ t: Double) { command(["seek", String(item.start + t), "absolute+exact"]) }

    func stop() {
        statsTimer?.invalidate()
        if let mpv { mpv_set_wakeup_callback(mpv, nil, nil); mpv_terminate_destroy(mpv) }
        mpv = nil
        if let w = warpShader { try? FileManager.default.removeItem(atPath: w) }
    }
}

struct MpvVideo: UIViewRepresentable {
    let model: MpvModel
    func makeUIView(context: Context) -> MpvLayerView { model.view }
    func updateUIView(_ v: MpvLayerView, context: Context) {}
}

struct MpvPlayerScreen: View {
    @EnvironmentObject var library: Library
    let item: PlayItem
    @State private var model: MpvModel?
    @State private var controls = true

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let model {
                MpvVideo(model: model).ignoresSafeArea()
                    .onTapGesture { withAnimation { controls.toggle() } }
                if controls { MpvOverlay(model: model) }
            }
        }
        .navigationTitle(item.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(controls ? .visible : .hidden, for: .navigationBar)
        .toolbar(.hidden, for: .tabBar)
        .onAppear { if model == nil { model = MpvModel(item: item, interpolation: library.interpolation) } }
        .onDisappear { model?.stop() }
    }
}

struct MpvOverlay: View {
    @ObservedObject var model: MpvModel
    @State private var scrubbing = false
    @State private var value: Double = 0
    var body: some View {
        VStack {
            HStack(spacing: 8) {
                Toggle(isOn: $model.stabilize) { Label("Stabilized", systemImage: "gyroscope") }
                    .toggleStyle(.button).disabled(!model.warpReady)
                Text("mpv").font(.caption.bold()).padding(.horizontal, 6).padding(.vertical, 2)
                    .background(Capsule().fill(.orange.opacity(0.8)))
                Spacer()
            }
            .font(.footnote)
            Text(model.status).font(.caption2).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
            Text(model.stats).font(.caption2.monospacedDigit()).foregroundStyle(.yellow).frame(maxWidth: .infinity, alignment: .leading)
            Spacer()
            HStack(spacing: 12) {
                Button { model.togglePlay() } label: { Image(systemName: model.playing ? "pause.fill" : "play.fill").font(.title2) }
                Text(durationText(scrubbing ? value : model.time)).font(.caption.monospacedDigit())
                Slider(value: Binding(get: { scrubbing ? value : model.time }, set: { value = $0; model.seek($0) }),
                       in: 0...max(model.length, 0.1)) { editing in scrubbing = editing }
                Text(durationText(model.length)).font(.caption.monospacedDigit())
            }
        }
        .padding()
        .background(LinearGradient(colors: [.black.opacity(0.5), .clear, .clear, .black.opacity(0.6)], startPoint: .top, endPoint: .bottom).allowsHitTesting(false))
        .tint(.white)
    }
}
