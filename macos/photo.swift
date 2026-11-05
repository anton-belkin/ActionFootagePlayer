// afp-photo: a HEIC with a gain map from a still's two renditions, for gyroflow-export --photo on macOS.
//
//   afp-photo --width W --height H --sdr <file> [--hdr <file>] --out <file.heic> [--quality 0-1]
//             [--taken "YYYY:MM:DD HH:MM:SS"] [--make <name>] [--model <name>]
//
// The renditions are 16-bit RGB, little-endian, row by row (vs-gyroflow's photo::write_rgb16): --sdr the SDR look as
// an sRGB screen shows it (the image every app shows), --hdr the HDR look in HLG (BT.2100). Core Image (macOS 15)
// writes the SDR one as the image and computes the gain map to the HDR one (ISO 21496-1), as Photos and the iPhone
// read it. (ImageIO's own kCGImageDestinationEncodeGenerateGainMapWithBaseImage refuses a CGImage as the base, as of
// macOS 27.) Without --hdr, or before macOS 15, a plain HEIC of the SDR one.

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
      let sdrPath = args["sdr"], let outPath = args["out"] else {
    fail("usage: afp-photo --width W --height H --sdr <file> [--hdr <file>] --out <file.heic>")
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

let srgb = CGColorSpace(name: CGColorSpace.sRGB)!
let sdr = CIImage(cgImage: image(sdrPath, CGColorSpace.sRGB)).settingProperties(props)
var options: [CIImageRepresentationOption: Any] = [
    CIImageRepresentationOption(rawValue: kCGImageDestinationLossyCompressionQuality as String): Double(args["quality"] ?? "") ?? 0.9,
]
if let hdrPath = args["hdr"], #available(macOS 15, *) {
    options[.hdrImage] = CIImage(cgImage: image(hdrPath, CGColorSpace.itur_2100_HLG))
}
do {
    try CIContext().writeHEIFRepresentation(of: sdr, to: URL(fileURLWithPath: outPath), format: .RGBA8, colorSpace: srgb, options: options)
} catch {
    fail("\(outPath): \(error.localizedDescription)")
}
