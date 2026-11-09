// The player: AVFoundation decodes (the video's planes as they are, 10 bits), the motion library builds the clip's
// warp (warp.glsl's exact pass in Metal) and its table, and each frame is drawn by one pass: the planes read at the
// warped coordinate, made R'G'B', through the log curve's LUT, into an HLG (or PQ, or SDR) layer.
import SwiftUI
import AVFoundation
import Metal
import QuartzCore

struct PlayItem: Hashable {
    let url: URL
    let title: String
    var start: Double = 0
    /// A moment: its length (it plays that part, looped)
    var duration: Double? = nil
}

/// The log curve the clip is in, by its camera: the LUT that makes it HLG
enum LogCurve: String {
    case dlogm = "dlogm_hlg", gplog = "gplog_hlg", none = ""
    static func guess(_ url: URL) -> LogCurve {
        let n = url.lastPathComponent.uppercased()
        if n.hasPrefix("DJI_") { return .dlogm }
        if n.hasPrefix("GX") || n.hasPrefix("GH") || n.hasPrefix("GOPR") { return .gplog }
        return .none
    }
}

@MainActor
final class PlayerModel: ObservableObject {
    let item: PlayItem
    let player = AVPlayer()
    let output = AVPlayerItemVideoOutput(pixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
        kCVPixelBufferMetalCompatibilityKey as String: true,
    ])
    let renderer: Renderer?
    @Published var status = "Opening…"
    @Published var stabilize = true { didSet { renderer?.stabilize = stabilize; layoutChanged += 1 } }
    @Published var log = true { didSet { renderer?.useLUT = log } }
    @Published var playing = false
    @Published var time: Double = 0
    @Published var length: Double = 1
    @Published var layoutChanged = 0
    @Published var warpReady = false
    let curve: LogCurve
    private var timeObserver: Any?

    init(item: PlayItem, interpolation: String) {
        self.item = item
        curve = LogCurve.guess(item.url)
        renderer = Renderer()
        renderer?.output = output
        log = curve != .none
        renderer?.useLUT = log
        if curve != .none { renderer?.loadLUT(named: curve.rawValue) }
        let pi = AVPlayerItem(url: item.url)
        pi.add(output)
        player.replaceCurrentItem(with: pi)
        player.actionAtItemEnd = .pause
        let start = item.start
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 10), queue: .main) { [weak self] t in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.time = t.seconds - start
                if let d = self.item.duration, self.time >= d { self.player.seek(to: CMTime(seconds: start, preferredTimescale: 6000), toleranceBefore: .zero, toleranceAfter: .zero) }
                self.playing = self.player.rate != 0
            }
        }
        Task { await prepare(interpolation: interpolation) }
    }

    func stop() {
        player.pause()
        if let timeObserver { player.removeTimeObserver(timeObserver) }
        timeObserver = nil
        renderer?.stop()
    }

    func prepare(interpolation: String) async {
        let asset = AVURLAsset(url: item.url)
        guard let track = try? await asset.loadTracks(withMediaType: .video).first,
              let size = try? await track.load(.naturalSize),
              let dur = try? await asset.load(.duration) else { status = "Can't read this video"; return }
        length = item.duration ?? max(dur.seconds - item.start, 0.1)
        if item.start > 0 { await player.seek(to: CMTime(seconds: item.start, preferredTimescale: 6000), toleranceBefore: .zero, toleranceAfter: .zero) }
        player.play()
        guard renderer != nil else { status = "No Metal here"; return }
        status = "Reading motion data…"
        // The player's JSON: its Gyroflow project beside it, if it has one, else the video with its own motion data
        let base = item.url.deletingPathExtension()
        let gyroflow = base.appendingPathExtension("gyroflow")
        let project = FileManager.default.fileExists(atPath: gyroflow.path) ? gyroflow : item.url
        let json: [String: Any] = ["project": project.path, "input_width": Int(size.width), "input_height": Int(size.height),
                                   "interpolation": interpolation]
        let text = String(data: try! JSONSerialization.data(withJSONObject: json), encoding: .utf8)!
        let t0 = Date()
        let result = await Task.detached(priority: .userInitiated) { () -> (String?, String?, Warp?) in
            guard let w = afp_warp_build(text) else { return (nil, "no result", nil) }
            defer { afp_warp_free(w) }
            let p = w.pointee
            if let e = p.error { return (nil, String(cString: e), nil) }
            let note = p.note.map { String(cString: $0) } ?? ""
            let table = Array(UnsafeBufferPointer(start: p.table, count: Int(p.table_w * p.table_h * 4)))
            let warp = Warp(msl: String(cString: p.msl!), table: table, tableW: Int(p.table_w), tableH: Int(p.table_h),
                            out: CGSize(width: Int(p.out_w), height: Int(p.out_h)), fps: p.fps, stabilized: note.isEmpty)
            return (note, nil, warp)
        }.value
        if let err = result.1 { status = "Not stabilized: \(err)"; stabilize = false; return }
        guard let warp = result.2 else { return }
        do {
            try renderer?.setWarp(warp)
            warpReady = warp.stabilized
            if !warp.stabilized { stabilize = false }
            let note = result.0 ?? ""
            status = note.isEmpty ? String(format: "Stabilized here (motion read in %.1f s)", Date().timeIntervalSince(t0)) : note
        } catch {
            status = "Shader: \(error.localizedDescription)"
            stabilize = false
        }
        layoutChanged += 1
    }

    func togglePlay() {
        if player.rate != 0 { player.pause() } else {
            if time >= length - 0.05 { seek(0) }
            player.play()
        }
        playing = player.rate != 0
    }

    func seek(_ t: Double) {
        player.seek(to: CMTime(seconds: item.start + t, preferredTimescale: 6000), toleranceBefore: .zero, toleranceAfter: .zero)
    }
}

struct Warp {
    let msl: String
    let table: [Float]
    let tableW: Int
    let tableH: Int
    let out: CGSize
    let fps: Double
    let stabilized: Bool
}

struct Uniforms {
    var ptsLogStab = SIMD4<Float>(0, 0, 0, 0)
    var sizes = SIMD4<Float>(0, 0, 0, 0)
    var cam = SIMD4<Float>(0, 0, 0, 1)
    var camZoomRange = SIMD4<Float>(1, 1, 0, 0)
    var yuv = matrix_identity_float4x4
}

/// Y'CbCr (range removed, chroma centred) -> R'G'B', for the clip's matrix (column-major: Y, Cb, Cr, 1)
func yuvMatrix(_ name: CFString?) -> simd_float4x4 {
    var kr: Float = 0.2627, kb: Float = 0.0593          // BT.2020
    if let n = name {
        if CFEqual(n, kCVImageBufferYCbCrMatrix_ITU_R_709_2) { kr = 0.2126; kb = 0.0722 }
        else if CFEqual(n, kCVImageBufferYCbCrMatrix_ITU_R_601_4) { kr = 0.299; kb = 0.114 }
    }
    let kg = 1 - kr - kb
    let rCr = 2 * (1 - kr), bCb = 2 * (1 - kb)
    let gCb = -bCb * kb / kg, gCr = -rCr * kr / kg
    return simd_float4x4(columns: (SIMD4(1, 1, 1, 0), SIMD4(0, gCb, bCb, 0), SIMD4(rCr, gCr, 0, 0), SIMD4(0, 0, 0, 1)))
}

private let vertexSource = """
#include <metal_stdlib>
using namespace metal;
struct AfpVOut { float4 position [[position]]; float2 pos [[user(loc0)]]; };
vertex AfpVOut afp_vertex(uint vid [[vertex_id]]) {
    float2 p = float2((vid << 1) & 2, vid & 2);
    AfpVOut o;
    o.position = float4(p * 2.0 - 1.0, 0.0, 1.0);
    o.pos = float2(p.x, 1.0 - p.y);
    return o;
}
"""

final class Renderer {
    let device: MTLDevice
    let queue: MTLCommandQueue
    var textureCache: CVMetalTextureCache?
    let sampler: MTLSamplerState
    var output: AVPlayerItemVideoOutput?
    var plainPipeline: MTLRenderPipelineState?
    var warpPipeline: MTLRenderPipelineState?
    var tableTexture: MTLTexture?
    var lutTexture: MTLTexture?
    var lutSize: Float = 0
    var warpOut: CGSize?
    var stabilize = true
    var useLUT = true
    private var current: CVPixelBuffer?
    private var currentPTS: Double = 0
    private let vertexFunction: MTLFunction
    let lock = NSLock()

    init?() {
        guard let d = MTLCreateSystemDefaultDevice(), let q = d.makeCommandQueue() else { return nil }
        device = d
        queue = q
        CVMetalTextureCacheCreate(nil, nil, d, nil, &textureCache)
        let sd = MTLSamplerDescriptor()
        sd.minFilter = .linear
        sd.magFilter = .linear
        sd.sAddressMode = .clampToEdge
        sd.tAddressMode = .clampToEdge
        sd.rAddressMode = .clampToEdge
        sampler = d.makeSamplerState(descriptor: sd)!
        guard let lib = try? d.makeLibrary(source: vertexSource, options: nil), let vf = lib.makeFunction(name: "afp_vertex") else { return nil }
        vertexFunction = vf
        // A 1x1 table and LUT until the real ones come, so the plain pass always has its bindings
        tableTexture = makeTexture2D(width: 1, height: 1, data: [0, 0, 0, 0])
        if let msl = afp_plain_msl() {
            let s = String(cString: msl)
            afp_string_free(msl)
            plainPipeline = try? pipeline(s)
        }
    }

    func makeTexture2D(width: Int, height: Int, data: [Float]) -> MTLTexture? {
        let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: width, height: height, mipmapped: false)
        td.usage = .shaderRead
        guard let t = device.makeTexture(descriptor: td) else { return nil }
        data.withUnsafeBytes { t.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: $0.baseAddress!, bytesPerRow: width * 16) }
        return t
    }

    func pipeline(_ fragmentMSL: String) throws -> MTLRenderPipelineState {
        let lib = try device.makeLibrary(source: fragmentMSL, options: nil)
        guard let name = lib.functionNames.first(where: { lib.makeFunction(name: $0)?.functionType == .fragment }),
              let ff = lib.makeFunction(name: name) else { throw NSError(domain: "afp", code: 1, userInfo: [NSLocalizedDescriptionKey: "no fragment function"]) }
        let pd = MTLRenderPipelineDescriptor()
        pd.vertexFunction = vertexFunction
        pd.fragmentFunction = ff
        pd.colorAttachments[0].pixelFormat = .rgba16Float
        return try device.makeRenderPipelineState(descriptor: pd)
    }

    func setWarp(_ w: Warp) throws {
        let p = try pipeline(w.msl)
        let t = makeTexture2D(width: w.tableW, height: w.tableH, data: w.table)
        lock.lock()
        warpPipeline = p
        tableTexture = t
        warpOut = w.stabilized ? w.out : nil
        lock.unlock()
    }

    /// A .cube from the app's resources as a 3D texture (R across, G down, B deep, as the file lists them)
    func loadLUT(named name: String) {
        guard let url = Bundle.main.url(forResource: name, withExtension: "cube"),
              let text = try? String(contentsOf: url, encoding: .utf8) else { return }
        var n = 0
        var values: [Float16] = []
        for line in text.split(whereSeparator: \.isNewline) {
            if line.hasPrefix("LUT_3D_SIZE") { n = Int(line.split(separator: " ").last ?? "") ?? 0; values.reserveCapacity(n * n * n * 4); continue }
            guard let c = line.first, c.isNumber || c == "-" || c == "." else { continue }
            let parts = line.split(separator: " ").compactMap { Float($0) }
            if parts.count == 3 { values.append(contentsOf: [Float16(parts[0]), Float16(parts[1]), Float16(parts[2]), 1]) }
        }
        guard n > 1, values.count == n * n * n * 4 else { return }
        let td = MTLTextureDescriptor()
        td.textureType = .type3D
        td.pixelFormat = .rgba16Float
        td.width = n; td.height = n; td.depth = n
        td.usage = .shaderRead
        guard let t = device.makeTexture(descriptor: td) else { return }
        values.withUnsafeBytes {
            t.replace(region: MTLRegionMake3D(0, 0, 0, n, n, n), mipmapLevel: 0, slice: 0, withBytes: $0.baseAddress!, bytesPerRow: n * 8, bytesPerImage: n * n * 8)
        }
        lutTexture = t
        lutSize = Float(n)
    }

    /// The picture's shape: the stabilized output's, or the video's
    var aspect: CGSize? {
        lock.lock(); defer { lock.unlock() }
        if stabilize, let o = warpOut { return o }
        guard let pb = current else { return nil }
        return CGSize(width: CVPixelBufferGetWidth(pb), height: CVPixelBufferGetHeight(pb))
    }

    /// What the shader makes of its output and the layer's colors: HLG (the LUT's, or the video's own) or PQ made
    /// linear light for EDR (extended linear BT.2020), else the video's SDR as it is
    func outputMode() -> (Float, CGColorSpace?) {
        let edr = CGColorSpace(name: CGColorSpace.extendedLinearITUR_2020)
        guard let pb = current else { return (1, edr) }
        if useLUT && lutTexture != nil { return (1, edr) }
        let tf = CVBufferCopyAttachment(pb, kCVImageBufferTransferFunctionKey, nil) as! CFString?
        if let tf, CFEqual(tf, kCVImageBufferTransferFunction_ITU_R_2100_HLG) { return (1, edr) }
        if let tf, CFEqual(tf, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ) { return (2, edr) }
        return (0, CGColorSpace(name: CGColorSpace.itur_709))
    }

    func stop() { output = nil }

    /// Draws the frame due now into the layer; false when there was nothing to draw yet
    @discardableResult
    func draw(into layer: CAMetalLayer, headroom: CGFloat) -> Bool {
        guard let output else { return false }
        let t = output.itemTime(forHostTime: CACurrentMediaTime())
        if output.hasNewPixelBuffer(forItemTime: t) {
            var shown = CMTime.zero
            if let pb = output.copyPixelBuffer(forItemTime: t, itemTimeForDisplay: &shown) {
                lock.lock(); current = pb; currentPTS = shown.seconds; lock.unlock()
            }
        }
        lock.lock()
        let pb = current, pts = currentPTS
        let warp = stabilize && warpOut != nil ? warpPipeline : nil
        let pipe = warp ?? plainPipeline ?? warpPipeline
        let table = tableTexture
        lock.unlock()
        guard let pb, let pipe, let cache = textureCache, layer.drawableSize.width > 0 else { return false }
        let (mode, cs) = outputMode()
        if let cs, layer.colorspace?.name != cs.name { layer.colorspace = cs }
        let w = CVPixelBufferGetWidthOfPlane(pb, 0), h = CVPixelBufferGetHeightOfPlane(pb, 0)
        var yRef: CVMetalTexture?, cRef: CVMetalTexture?
        CVMetalTextureCacheCreateTextureFromImage(nil, cache, pb, nil, .r16Unorm, w, h, 0, &yRef)
        CVMetalTextureCacheCreateTextureFromImage(nil, cache, pb, nil, .rg16Unorm, CVPixelBufferGetWidthOfPlane(pb, 1), CVPixelBufferGetHeightOfPlane(pb, 1), 1, &cRef)
        guard let yRef, let cRef, let yTex = CVMetalTextureGetTexture(yRef), let cTex = CVMetalTextureGetTexture(cRef),
              let drawable = layer.nextDrawable(), let cmd = queue.makeCommandBuffer() else { return false }
        let rp = MTLRenderPassDescriptor()
        rp.colorAttachments[0].texture = drawable.texture
        rp.colorAttachments[0].loadAction = .clear
        rp.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        rp.colorAttachments[0].storeAction = .store
        guard let enc = cmd.makeRenderCommandEncoder(descriptor: rp) else { return false }
        var u = Uniforms()
        u.ptsLogStab = SIMD4(Float(pts), useLUT && lutTexture != nil ? 1 : 0, warp != nil ? 1 : 0, lutSize)
        u.sizes = SIMD4(Float(layer.drawableSize.width), Float(layer.drawableSize.height), Float(w), Float(h))
        u.camZoomRange = SIMD4(1, 1, mode, Float(headroom))
        u.yuv = yuvMatrix(CVBufferCopyAttachment(pb, kCVImageBufferYCbCrMatrixKey, nil) as! CFString?)
        enc.setRenderPipelineState(pipe)
        enc.setFragmentTexture(yTex, index: 0)
        enc.setFragmentTexture(cTex, index: 1)
        enc.setFragmentTexture(table, index: 2)
        enc.setFragmentTexture(lutTexture ?? dummyLUT, index: 3)
        enc.setFragmentSamplerState(sampler, index: 0)
        enc.setFragmentBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        enc.endEncoding()
        cmd.present(drawable)
        cmd.addCompletedHandler { _ in _ = yRef; _ = cRef }
        cmd.commit()
        return true
    }

    lazy var dummyLUT: MTLTexture? = {
        let td = MTLTextureDescriptor()
        td.textureType = .type3D
        td.pixelFormat = .rgba16Float
        td.width = 2; td.height = 2; td.depth = 2
        td.usage = .shaderRead
        return device.makeTexture(descriptor: td)
    }()
}

/// A view whose layer is the Metal layer, fitted to the picture's shape, redrawn on the display's beat
final class MetalVideoView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }
    var metalLayer: CAMetalLayer { layer as! CAMetalLayer }
    weak var renderer: Renderer?
    private var link: CADisplayLink?
    private var lastAspect: CGSize?

    func start(_ r: Renderer) {
        renderer = r
        metalLayer.device = r.device
        metalLayer.pixelFormat = .rgba16Float
        metalLayer.framebufferOnly = true
        metalLayer.wantsExtendedDynamicRangeContent = true
        metalLayer.colorspace = CGColorSpace(name: CGColorSpace.extendedLinearITUR_2020)
        backgroundColor = .black
        let l = CADisplayLink(target: self, selector: #selector(tick))
        l.preferredFrameRateRange = CAFrameRateRange(minimum: 24, maximum: 120, preferred: 60)
        l.add(to: .main, forMode: .common)
        link = l
    }

    func stop() { link?.invalidate(); link = nil }

    @objc private func tick() {
        guard let r = renderer else { return }
        if r.aspect != lastAspect { setNeedsLayout() }
        r.draw(into: metalLayer, headroom: window?.windowScene?.screen.currentEDRHeadroom ?? 1)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // The drawable is the picture's rectangle on screen: the warp is the scaler, to the screen's pixels
        guard let superview else { return }
        let bounds = superview.bounds
        let a = renderer?.aspect ?? CGSize(width: 16, height: 9)
        lastAspect = renderer?.aspect
        let scale = min(bounds.width / max(a.width, 1), bounds.height / max(a.height, 1))
        let size = CGSize(width: floor(a.width * scale), height: floor(a.height * scale))
        frame = CGRect(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
        let s = window?.screen.nativeScale ?? UIScreen.main.nativeScale
        metalLayer.contentsScale = s
        metalLayer.drawableSize = CGSize(width: size.width * s, height: size.height * s)
    }
}

final class VideoHost: UIView {
    let video = MetalVideoView()
    override init(frame: CGRect) { super.init(frame: frame); backgroundColor = .black; addSubview(video) }
    required init?(coder: NSCoder) { fatalError() }
    override func layoutSubviews() { super.layoutSubviews(); video.setNeedsLayout(); video.layoutIfNeeded() }
}

struct MetalVideo: UIViewRepresentable {
    let model: PlayerModel
    let layoutTick: Int
    func makeUIView(context: Context) -> VideoHost {
        let v = VideoHost()
        if let r = model.renderer { v.video.start(r) }
        return v
    }
    func updateUIView(_ v: VideoHost, context: Context) { v.setNeedsLayout() }
    static func dismantleUIView(_ v: VideoHost, coordinator: ()) { v.video.stop() }
}

struct PlayerScreen: View {
    @EnvironmentObject var library: Library
    let item: PlayItem
    @State private var model: PlayerModel?
    @State private var controls = true
    @State private var scrubbing = false
    @State private var scrubValue: Double = 0

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let model {
                MetalVideo(model: model, layoutTick: model.layoutChanged).ignoresSafeArea()
                    .onTapGesture { withAnimation { controls.toggle() } }
                if controls { overlay(model) }
            }
        }
        .navigationTitle(item.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(controls ? .visible : .hidden, for: .navigationBar)
        .toolbar(.hidden, for: .tabBar)
        .onAppear { if model == nil { model = PlayerModel(item: item, interpolation: library.interpolation) } }
        .onDisappear { model?.stop() }
    }

    @ViewBuilder func overlay(_ m: PlayerModel) -> some View {
        PlayerOverlay(model: m)
    }
}

struct PlayerOverlay: View {
    @ObservedObject var model: PlayerModel
    @State private var scrubbing = false
    @State private var value: Double = 0

    var body: some View {
        VStack {
            HStack(spacing: 8) {
                Toggle(isOn: $model.stabilize) { Label("Stabilized", systemImage: "gyroscope") }
                    .toggleStyle(.button).disabled(!model.warpReady)
                if model.curve != .none {
                    Toggle(isOn: $model.log) { Label(model.curve == .dlogm ? "D-Log M → HDR" : "GP-Log → HDR", systemImage: "camera.filters") }
                        .toggleStyle(.button)
                }
                Spacer()
            }
            .font(.footnote)
            Text(model.status).font(.caption2).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
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
