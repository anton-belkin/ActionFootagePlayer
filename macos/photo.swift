// afp-photo: a HEIC with a gain map from a still's two renditions, for gyroflow-export --photo on macOS.
//
//   afp-photo --width W --height H [--sdr <file>] [--hdr <file>] [--out <file.heic>] [--sdr-out <file>]
//             [--quality 0-1] [--taken "YYYY:MM:DD HH:MM:SS"] [--make <name>] [--model <name>] [--content-id <UUID>]
//             [--link <json>] [--gps <lat,lon[,alt]>]
//   afp-photo --live-video <in.mov> --live-out <out.mov> --content-id <UUID> --still-time <seconds> [--gps <lat,lon[,alt]>]
//   afp-photo --set-meta <file.heic> [--link <json>] [--tags <json list>]
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
//
// Live Photos: the HEIC and a MOV, paired by the same content identifier (a UUID). --content-id puts it in the
// HEIC's Apple maker note (key 17, as an iPhone writes it); --live-video copies a MOV as it is (no re-encoding: HLG
// and Dolby Vision stay) into --live-out with it as com.apple.quicktime.content.identifier, and a timed metadata
// track with com.apple.quicktime.still-image-time at --still-time, the photo's frame. Photos and the iPhone take the
// two as one Live Photo by them.
//
// --link: the photo's link to its clip, as XMP (afp:link in the player's namespace, https://actionfootageplayer.ch/ns/1.0/,
// as vs-gyroflow's photo.rs writes it in a JPEG): added once the HEIC is written, its image data copied as it is
// (ImageIO's CGImageDestinationCopyImageSource), the gain map and the Live Photo's identifier kept.
//
// --gps: where the photo was taken (degrees, metres above sea level), from the clip's GPS or its location in the
// Library (vs-gyroflow's geotag::photo_place): the HEIC's GPS dictionary (Exif GPS), and a Live Photo's MOV's
// com.apple.quicktime.location.ISO6709, as an iPhone writes them; Photos puts the pair on its map by them.

import Foundation
import ImageIO
import CoreGraphics
import CoreImage
import AVFoundation

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

// --gps "lat,lon[,alt]": where the photo was taken (degrees, metres)
let gps: (lat: Double, lon: Double, alt: Double?)? = args["gps"].flatMap { s in
    let v = s.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
    guard v.count >= 2, abs(v[0]) <= 90, abs(v[1]) <= 180 else { return nil }
    return (v[0], v[1], v.count > 2 ? v[2] : nil)
}
// --set-meta <file.heic> [--link <json>] [--tags <json list>]: a photo's link and tags (vs-gyroflow's afp-meta asks),
// written into its XMP (afp:link, dc:subject), its image data copied as it is: the gain map and the Live Photo's
// identifier kept. An empty list takes the tags out
if let path = args["set-meta"] {
    let url = URL(fileURLWithPath: path)
    let tmp = url.deletingLastPathComponent().appendingPathComponent("." + url.lastPathComponent + ".meta")
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil), let type = CGImageSourceGetType(src),
          let dst = CGImageDestinationCreateWithURL(tmp as CFURL, type, 1, nil) else { fail("\(path): can't be read as a photo") }
    let md = CGImageMetadataCreateMutable()
    let afp = "https://actionfootageplayer.ch/ns/1.0/", dc = "http://purl.org/dc/elements/1.1/"
    guard CGImageMetadataRegisterNamespaceForPrefix(md, afp as CFString, "afp" as CFString, nil),
          CGImageMetadataRegisterNamespaceForPrefix(md, dc as CFString, "dc" as CFString, nil) else { fail("\(path): namespaces") }
    if let link = args["link"] {
        guard CGImageMetadataSetValueWithPath(md, nil, "afp:link" as CFString, link as CFString) else { fail("\(path): the link") }
    }
    if let t = args["tags"] {
        guard let data = t.data(using: .utf8), let list = (try? JSONSerialization.jsonObject(with: data)) as? [String] else { fail("--tags takes a JSON list") }
        // Merged, the list replaces the one there (empty: an empty list, no tags)
        guard let tag = CGImageMetadataTagCreate(dc as CFString, "dc" as CFString, "subject" as CFString, .arrayUnordered, list as CFArray),
              CGImageMetadataSetTagWithPath(md, nil, "dc:subject" as CFString, tag) else { fail("\(path): the tags") }
    }
    var err: Unmanaged<CFError>?
    guard CGImageDestinationCopyImageSource(dst, src, [kCGImageDestinationMetadata: md, kCGImageDestinationMergeMetadata: true] as CFDictionary, &err) else {
        try? FileManager.default.removeItem(at: tmp)
        fail("\(path): not written (\(err.map { String(describing: $0.takeRetainedValue()) } ?? "?"))")
    }
    do { _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp) } catch { fail("\(path): \(error.localizedDescription)") }
    exit(0)
}

// A Live Photo's video: the MOV copied with the content identifier and the still's time
func liveVideo(from input: URL, to output: URL, id: String, still: Double) {
    let asset = AVURLAsset(url: input)
    let reader: AVAssetReader, writer: AVAssetWriter
    do {
        reader = try AVAssetReader(asset: asset)
        try? FileManager.default.removeItem(at: output)
        writer = try AVAssetWriter(outputURL: output, fileType: .mov)
    } catch { fail("\(input.path): \(error.localizedDescription)") }
    let identifier = AVMutableMetadataItem()
    identifier.keySpace = .quickTimeMetadata
    identifier.key = "com.apple.quicktime.content.identifier" as NSString
    identifier.value = id as NSString
    identifier.dataType = kCMMetadataBaseDataType_UTF8 as String
    writer.metadata = [identifier]
    // Where it was taken, as an iPhone writes it (ISO 6709: "+46.10200+007.91928+1780.429/")
    if let gps {
        let location = AVMutableMetadataItem()
        location.keySpace = .quickTimeMetadata
        location.key = "com.apple.quicktime.location.ISO6709" as NSString
        location.value = (String(format: "%+09.5f%+010.5f", gps.lat, gps.lon) + (gps.alt.map { String(format: "%+.3f", $0) } ?? "") + "/") as NSString
        location.dataType = kCMMetadataBaseDataType_UTF8 as String
        writer.metadata.append(location)
    }

    var pairs: [(AVAssetReaderTrackOutput, AVAssetWriterInput)] = []
    for track in asset.tracks where track.mediaType == .video || track.mediaType == .audio {
        let out = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
        out.alwaysCopiesSampleData = false
        guard reader.canAdd(out) else { fail("\(input.path): can't read its \(track.mediaType.rawValue) track") }
        reader.add(out)
        let hint = track.formatDescriptions.first.map { $0 as! CMFormatDescription }
        let inp = AVAssetWriterInput(mediaType: track.mediaType, outputSettings: nil, sourceFormatHint: hint)
        inp.expectsMediaDataInRealTime = false
        // The video keeps its own time scale (AVAssetWriter's default, 1/600 s, rounds 59.94 fps's frame times unevenly)
        if track.mediaType == .video {
            inp.transform = track.preferredTransform
            if track.naturalTimeScale > 0 { inp.mediaTimeScale = track.naturalTimeScale }
        }
        guard writer.canAdd(inp) else { fail("\(output.path): can't write a \(track.mediaType.rawValue) track") }
        writer.add(inp)
        pairs.append((out, inp))
    }
    guard pairs.contains(where: { $0.1.mediaType == .video }) else { fail("\(input.path): no video") }

    // The still's time: one timed metadata item, as the iPhone writes it
    let spec: [String: Any] = [
        kCMMetadataFormatDescriptionMetadataSpecificationKey_Identifier as String: "mdta/com.apple.quicktime.still-image-time",
        kCMMetadataFormatDescriptionMetadataSpecificationKey_DataType as String: kCMMetadataBaseDataType_SInt8 as String,
    ]
    var desc: CMFormatDescription?
    CMMetadataFormatDescriptionCreateWithMetadataSpecifications(allocator: kCFAllocatorDefault, metadataType: kCMMetadataFormatType_Boxed,
                                                                metadataSpecifications: [spec] as CFArray, formatDescriptionOut: &desc)
    let metaInput = AVAssetWriterInput(mediaType: .metadata, outputSettings: nil, sourceFormatHint: desc)
    metaInput.expectsMediaDataInRealTime = false
    let adaptor = AVAssetWriterInputMetadataAdaptor(assetWriterInput: metaInput)
    writer.add(metaInput)

    guard writer.startWriting() else { fail("\(output.path): \(writer.error?.localizedDescription ?? "can't write it")") }
    guard reader.startReading() else { fail("\(input.path): \(reader.error?.localizedDescription ?? "can't read it")") }
    writer.startSession(atSourceTime: .zero)

    let item = AVMutableMetadataItem()
    item.keySpace = .quickTimeMetadata
    item.key = "com.apple.quicktime.still-image-time" as NSString
    item.value = NSNumber(value: Int8(-1))
    item.dataType = kCMMetadataBaseDataType_SInt8 as String
    // Halfway through the photo's frame: the metadata track's time scale (1/600 s) can't round it into the frame before
    let video = asset.tracks(withMediaType: .video)[0]
    let frame = video.minFrameDuration.isValid && video.minFrameDuration.value > 0 ? video.minFrameDuration.seconds : 1.0 / 30
    let at = CMTime(seconds: max(0, still) + frame / 2, preferredTimescale: 60000)
    guard adaptor.append(AVTimedMetadataGroup(items: [item], timeRange: CMTimeRange(start: at, duration: CMTime(seconds: frame / 4, preferredTimescale: 60000)))) else {
        fail("\(output.path): the still's time: \(writer.error?.localizedDescription ?? "not written")")
    }
    metaInput.markAsFinished()

    let group = DispatchGroup()
    for (n, (out, inp)) in pairs.enumerated() {
        group.enter()
        inp.requestMediaDataWhenReady(on: DispatchQueue(label: "afp-photo.live.\(n)")) {
            while inp.isReadyForMoreMediaData {
                guard let sample = out.copyNextSampleBuffer(), inp.append(sample) else {
                    inp.markAsFinished()
                    group.leave()
                    return
                }
            }
        }
    }
    group.wait()
    if reader.status == .failed { fail("\(input.path): \(reader.error?.localizedDescription ?? "reading failed")") }
    let done = DispatchSemaphore(value: 0)
    writer.finishWriting { done.signal() }
    done.wait()
    guard writer.status == .completed else { fail("\(output.path): \(writer.error?.localizedDescription ?? "not written")") }
}

if let video = args["live-video"] {
    guard let out = args["live-out"], let id = args["content-id"], let still = Double(args["still-time"] ?? "") else {
        fail("usage: afp-photo --live-video <in.mov> --live-out <out.mov> --content-id <UUID> --still-time <seconds>")
    }
    liveVideo(from: URL(fileURLWithPath: video), to: URL(fileURLWithPath: out), id: id, still: still)
    if args["width"] == nil { exit(0) }
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
if let gps {
    var g: [CFString: Any] = [
        kCGImagePropertyGPSVersion: [2, 3, 0, 0],
        kCGImagePropertyGPSLatitude: abs(gps.lat), kCGImagePropertyGPSLatitudeRef: gps.lat < 0 ? "S" : "N",
        kCGImagePropertyGPSLongitude: abs(gps.lon), kCGImagePropertyGPSLongitudeRef: gps.lon < 0 ? "W" : "E",
    ]
    if let alt = gps.alt { g[kCGImagePropertyGPSAltitude] = abs(alt); g[kCGImagePropertyGPSAltitudeRef] = alt < 0 ? 1 : 0 }
    props[kCGImagePropertyGPSDictionary] = g
}
// A Live Photo's half: the identifier its MOV has too
if let id = args["content-id"] { props[kCGImagePropertyMakerAppleDictionary] = ["17": id] }
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

// The link, in the HEIC's XMP: the file copied with it merged in, then put in its place
if let link = args["link"] {
    let url = URL(fileURLWithPath: outPath)
    let tmp = url.deletingLastPathComponent().appendingPathComponent("." + url.lastPathComponent + ".link")
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil), let type = CGImageSourceGetType(src),
          let dst = CGImageDestinationCreateWithURL(tmp as CFURL, type, 1, nil) else { fail("\(outPath): can't add its link") }
    let md = CGImageMetadataCreateMutable()
    guard CGImageMetadataRegisterNamespaceForPrefix(md, "https://actionfootageplayer.ch/ns/1.0/" as CFString, "afp" as CFString, nil),
          CGImageMetadataSetValueWithPath(md, nil, "afp:link" as CFString, link as CFString) else { fail("\(outPath): the link") }
    var err: Unmanaged<CFError>?
    guard CGImageDestinationCopyImageSource(dst, src, [kCGImageDestinationMetadata: md, kCGImageDestinationMergeMetadata: true] as CFDictionary, &err) else {
        try? FileManager.default.removeItem(at: tmp)
        fail("\(outPath): its link not added (\(err.map { String(describing: $0.takeRetainedValue()) } ?? "?"))")
    }
    do { _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp) } catch { fail("\(outPath): \(error.localizedDescription)") }
}
