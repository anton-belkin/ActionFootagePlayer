// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-upscale: a neural upscaler (x4) for a photo's crop, through Core ML, for gyroflow-export --photo --upscale
// (docs/photo-upscaling.md). The models are Core ML conversions of Real-ESRGAN x4plus and SwinIR-L (real-world SR,
// GAN), made by tools/upscalers/convert.py, with a fixed input tile; they live in the app's Application Support
// folder, not in the app (models/, fetched on first use: upscale_models.rs).
//
//   afp-upscale --model <file.mlpackage|.mlmodelc> --width W --height H --in <rgb16> --out <rgb16>
//               [--overlap 16] [--units all|gpu|cpu]
//
// In and out: 16-bit RGB, little-endian, row by row (photo.rs's write_rgb16), the signal 0..1 as the model saw it in
// training (an sRGB picture: the SDR rendition). Out is 4W x 4H. The picture is cut into the model's tiles, each read
// with `overlap` pixels around it (clamped to the picture; a picture smaller than a tile padded by its edge pixels),
// and of each tile's output only its own part kept, as research_notes' run.py did with torch. A .mlpackage is compiled
// once, the .mlmodelc kept beside it. Progress on stderr: "progress <tiles done> <tiles>", then "done <seconds>".

import CoreML
import Foundation

func fail(_ s: String) -> Never {
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
    exit(1)
}
func say(_ s: String) { FileHandle.standardError.write((s + "\n").data(using: .utf8)!) }

var args = [String: String]()
var it = CommandLine.arguments.dropFirst().makeIterator()
while let a = it.next() {
    guard a.hasPrefix("--"), let v = it.next() else { fail("afp-upscale: bad argument \(a)") }
    args[String(a.dropFirst(2))] = v
}
guard let modelPath = args["model"], let input = args["in"], let output = args["out"],
      let w = Int(args["width"] ?? ""), let h = Int(args["height"] ?? ""), w > 0, h > 0 else {
    fail("usage: afp-upscale --model <file> --width W --height H --in <rgb16> --out <rgb16> [--overlap 16] [--units all|gpu|cpu]")
}
let overlap = Int(args["overlap"] ?? "16") ?? 16

// The model: compiled once (Core ML's compileModel), kept as <name>.mlmodelc next to the package
var url = URL(fileURLWithPath: modelPath)
if url.pathExtension == "mlpackage" || url.pathExtension == "mlmodel" {
    let compiled = url.deletingPathExtension().appendingPathExtension("mlmodelc")
    let fm = FileManager.default
    let stale = (try? fm.attributesOfItem(atPath: compiled.path)[.modificationDate] as? Date).flatMap { c in
        (try? fm.attributesOfItem(atPath: url.path)[.modificationDate] as? Date).map { $0 > c } } ?? true
    if stale {
        do {
            let tmp = try MLModel.compileModel(at: url)
            try? fm.removeItem(at: compiled)
            try fm.moveItem(at: tmp, to: compiled)
        } catch { fail("afp-upscale: \(url.lastPathComponent): \(error.localizedDescription)") }
    }
    url = compiled
}
let config = MLModelConfiguration()
switch args["units"] ?? "gpu" {
case "all": config.computeUnits = .all
case "cpu": config.computeUnits = .cpuOnly
default: config.computeUnits = .cpuAndGPU   // the Neural Engine compiles SwinIR-L for minutes, and isn't faster here
}
let model: MLModel
do { model = try MLModel(contentsOf: url, configuration: config) } catch { fail("afp-upscale: \(error.localizedDescription)") }
guard let inDesc = model.modelDescription.inputDescriptionsByName.first?.value,
      let shape = inDesc.multiArrayConstraint?.shape.map({ $0.intValue }), shape.count == 4, shape[1] == 3 else {
    fail("afp-upscale: not an image model (1x3xHxW in)")
}
let inName = model.modelDescription.inputDescriptionsByName.first!.key
let outName = model.modelDescription.outputDescriptionsByName.first!.key
let (th, tw) = (shape[2], shape[3])
let scale = 4

// The picture, as floats, planar
guard let raw = FileManager.default.contents(atPath: input), raw.count == w * h * 6 else { fail("afp-upscale: \(input) isn't \(w)x\(h) 16-bit RGB") }
var src = [Float](repeating: 0, count: 3 * w * h)
raw.withUnsafeBytes { (p: UnsafeRawBufferPointer) in
    let u = p.bindMemory(to: UInt16.self)
    for i in 0..<(w * h) { for c in 0..<3 { src[c * w * h + i] = Float(UInt16(littleEndian: u[i * 3 + c])) / 65535 } }
}
let (ow, oh) = (w * scale, h * scale)
var dst = [UInt16](repeating: 0, count: ow * oh * 3)

// Tiles: each one's own part `core` x `core` (the tile less the overlap on both sides), read with the overlap around it
func starts(_ size: Int, _ tile: Int) -> [(own: Int, end: Int, read: Int)] {
    let core = max(tile - 2 * overlap, 8)
    var out = [(Int, Int, Int)]()
    var a = 0
    while a < size {
        let b = min(a + core, size)
        // The tile read: around [a, b), inside the picture where it fits
        let read = size <= tile ? 0 : min(max(a - overlap, 0), size - tile)
        out.append((a, b, read))
        a = b
    }
    return out
}
let cols = starts(w, tw), rows = starts(h, th)
let total = cols.count * rows.count
let arr: MLMultiArray
do { arr = try MLMultiArray(shape: shape.map { NSNumber(value: $0) }, dataType: .float32) } catch { fail("afp-upscale: \(error)") }
let t0 = Date()
var done = 0
say("progress 0 \(total)")
for r in rows {
    for c in cols {
        // In: the tile from the picture, its edge pixels repeated past the picture's edge
        let p = arr.dataPointer.bindMemory(to: Float.self, capacity: 3 * th * tw)
        for ch in 0..<3 {
            for y in 0..<th {
                let sy = min(r.read + y, h - 1)
                for x in 0..<tw {
                    p[(ch * th + y) * tw + x] = src[ch * w * h + sy * w + min(c.read + x, w - 1)]
                }
            }
        }
        let out: MLFeatureProvider
        do {
            out = try autoreleasepool { try model.prediction(from: MLDictionaryFeatureProvider(dictionary: [inName: MLFeatureValue(multiArray: arr)])) }
        } catch { fail("afp-upscale: \(error.localizedDescription)") }
        guard let o = out.featureValue(for: outName)?.multiArrayValue else { fail("afp-upscale: no output") }
        let st = o.strides.map { $0.intValue }
        let get: (Int) -> Float
        switch o.dataType {
        case .float32: let q = o.dataPointer.bindMemory(to: Float.self, capacity: o.count); get = { q[$0] }
        case .float16: let q = o.dataPointer.bindMemory(to: Float16.self, capacity: o.count); get = { Float(q[$0]) }
        default: fail("afp-upscale: output of type \(o.dataType.rawValue)")
        }
        // Out: the tile's own part
        for y in (r.own * scale)..<(r.end * scale) {
            let ty = y - r.read * scale
            for x in (c.own * scale)..<(c.end * scale) {
                let tx = x - c.read * scale
                for ch in 0..<3 {
                    let v = get(ch * st[1] + ty * st[2] + tx * st[3])
                    dst[(y * ow + x) * 3 + ch] = UInt16(littleEndian: UInt16((min(max(v, 0), 1) * 65535).rounded()))
                }
            }
        }
        done += 1
        say("progress \(done) \(total)")
    }
}
let data = dst.withUnsafeBufferPointer { Data(buffer: $0) }
do { try data.write(to: URL(fileURLWithPath: output)) } catch { fail("afp-upscale: \(output): \(error.localizedDescription)") }
say(String(format: "done %.2f", Date().timeIntervalSince(t0)))
