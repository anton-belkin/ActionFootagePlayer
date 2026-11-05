// afp-photo: a HEIC with a gain map from a still's two renditions, for gyroflow-export --photo on macOS.
//
//   afp-photo --width W --height H [--sdr <file>] [--hdr <file>] [--out <file.heic>] [--sdr-out <file>]
//             [--quality 0-1] [--taken "YYYY:MM:DD HH:MM:SS"] [--make <name>] [--model <name>]
//
// The renditions are 16-bit RGB, little-endian, row by row (vs-gyroflow's photo::write_rgb16): --sdr the SDR look as
// an sRGB screen shows it (the image every app shows), --hdr the HDR look in HLG (BT.2100). Core Image (macOS 15)
// writes the SDR one as the image and computes the gain map to the HDR one (ISO 21496-1), as Photos and the iPhone
// read it. (ImageIO's own kCGImageDestinationEncodeGenerateGainMapWithBaseImage refuses a CGImage as the base, as of
// macOS 27.) Without --hdr, or before macOS 15, a plain HEIC of the SDR one.
//
// Without --sdr, the SDR image is the HLG one as macOS shows it on an SDR screen (the player's HDR look there): Core
// Image's own HLG -> SDR, the same codes as AVFoundation's (HLG 0.5 -> 0.386 and 0.75 -> 0.61 in BT.709). --sdr-out
// writes that SDR image as 16-bit RGB too, for gyroflow-export's JPEG; without --out, that's all it does.

import Foundation
import ImageIO
import CoreGraphics
import CoreImage

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(("afp-photo: " + message + "\n").data(using: .utf8)!)
    exit(1)
}

var args: [String: String] = [:]
var it = CommandLine.arguments.dropFirst().makeIterator()
while let a = it.next() {
    guard a.hasPrefix("--"), let v = it.next() else { fail("bad arguments") }
    args[String(a.dropFirst(2))] = v
}
guard let width = Int(args["width"] ?? ""), let height = Int(args["height"] ?? ""), width > 0, height > 0,
      args["sdr"] != nil || args["hdr"] != nil, args["out"] != nil || args["sdr-out"] != nil else {
    fail("usage: afp-photo --width W --height H [--sdr <file>] [--hdr <file>] [--out <file.heic>] [--sdr-out <file>]")
}

// 16-bit RGB to a 16-bit RGBX image in the color space given
func image(_ path: String, _ space: CFString) -> CGImage {
    guard let data = FileManager.default.contents(atPath: path), data.count == width * height * 6 else {
        fail("\(path): not \(width)x\(height) 16-bit RGB")
    }
    var rgbx = Data(count: width * height * 8)
    rgbx.withUnsafeMutableBytes { (dst: UnsafeMutableRawBufferPointer) in
        data.withUnsafeBytes { (src: UnsafeRawBufferPointer) in
            let s = src.bindMemory(to: UInt16.self), d = dst.bindMemory(to: UInt16.self)
            for i in 0..<(width * height) {
                d[i * 4] = s[i * 3]; d[i * 4 + 1] = s[i * 3 + 1]; d[i * 4 + 2] = s[i * 3 + 2]; d[i * 4 + 3] = 65535
            }
        }
    }
    guard let provider = CGDataProvider(data: rgbx as CFData), let cs = CGColorSpace(name: space),
          let img = CGImage(width: width, height: height, bitsPerComponent: 16, bitsPerPixel: 64, bytesPerRow: width * 8, space: cs,
                            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue | CGBitmapInfo.byteOrder16Little.rawValue),
                            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) else {
        fail("\(path): can't make an image of it")
    }
    return img
}

let srgb = CGColorSpace(name: CGColorSpace.sRGB)!
let context = CIContext()
let hdr = args["hdr"].map { CIImage(cgImage: image($0, CGColorSpace.itur_2100_HLG)) }

// The SDR image: the one given, else the HLG one as macOS tones it down, rendered into 16-bit sRGB
var sdr: CIImage
if let path = args["sdr"] {
    sdr = CIImage(cgImage: image(path, CGColorSpace.sRGB))
} else {
    var pixels = [UInt16](repeating: 0, count: width * height * 4)
    context.render(hdr!, toBitmap: &pixels, rowBytes: width * 8, bounds: CGRect(x: 0, y: 0, width: width, height: height), format: .RGBA16, colorSpace: srgb)
    if let out = args["sdr-out"] {
        var rgb = [UInt16](repeating: 0, count: width * height * 3)
        for i in 0..<(width * height) { rgb[i * 3] = pixels[i * 4]; rgb[i * 3 + 1] = pixels[i * 4 + 1]; rgb[i * 3 + 2] = pixels[i * 4 + 2] }
        guard FileManager.default.createFile(atPath: out, contents: Data(bytes: rgb, count: rgb.count * 2)) else { fail("\(out): can't write it") }
    }
    guard let provider = CGDataProvider(data: Data(bytes: pixels, count: pixels.count * 2) as CFData),
          let cg = CGImage(width: width, height: height, bitsPerComponent: 16, bitsPerPixel: 64, bytesPerRow: width * 8, space: srgb,
                           bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue | CGBitmapInfo.byteOrder16Little.rawValue),
                           provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) else { fail("the SDR image") }
    sdr = CIImage(cgImage: cg)
}
guard let outPath = args["out"] else { exit(0) }

// The metadata, as the image's properties: Core Image writes them
var props: [CFString: Any] = [:]
var exif: [CFString: Any] = [:]
if let taken = args["taken"] { exif[kCGImagePropertyExifDateTimeOriginal] = taken; exif[kCGImagePropertyExifDateTimeDigitized] = taken }
if !exif.isEmpty { props[kCGImagePropertyExifDictionary] = exif }
var tiff: [CFString: Any] = [:]
if let make = args["make"] { tiff[kCGImagePropertyTIFFMake] = make }
if let model = args["model"] { tiff[kCGImagePropertyTIFFModel] = model }
if let taken = args["taken"] { tiff[kCGImagePropertyTIFFDateTime] = taken }
if !tiff.isEmpty { props[kCGImagePropertyTIFFDictionary] = tiff }
sdr = sdr.settingProperties(props)

var options: [CIImageRepresentationOption: Any] = [
    CIImageRepresentationOption(rawValue: kCGImageDestinationLossyCompressionQuality as String): Double(args["quality"] ?? "") ?? 0.9,
]
if let hdr, #available(macOS 15, *) { options[.hdrImage] = hdr }
do {
    try context.writeHEIFRepresentation(of: sdr, to: URL(fileURLWithPath: outPath), format: .RGBA8, colorSpace: srgb, options: options)
} catch {
    fail("\(outPath): \(error.localizedDescription)")
}
