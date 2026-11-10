// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-thumbs: pictures of files as Finder draws them, for Action Footage Library's cards. The system's thumbnail
// service (QLThumbnailGenerator) decodes on the GPU, reads only a video's index and one keyframe (a few MB over SMB,
// not the clip), and keeps its own cache: a clip Finder has shown comes back at once.
//
//   afp-thumbs [--size 360]                   requests on stdin, one per line: <id>\t<file>\t<out.jpg>
//                                             replies on stdout, one per line, as each is done: <id>\tok | <id>\terr <why>
//   afp-thumbs [--size 360] --out <dir> <file>...   each file's picture to <dir>/<n>.jpg, a line per file:
//                                             <file>\t<ms>\tok|err (for timing by hand)
//
// Many at once: the service runs them side by side. It quits when stdin closes and all are answered. A file the
// service can't draw (no thumbnail, only an icon) is an error: the Library then has the player's ffmpeg make one.

import Foundation
import QuickLookThumbnailing
import ImageIO
import UniformTypeIdentifiers

var size = 360.0
var outDir: String? = nil
var files: [String] = []
var args = CommandLine.arguments.dropFirst().makeIterator()
while let a = args.next() {
    switch a {
    case "--size": size = Double(args.next() ?? "") ?? size
    case "--out": outDir = args.next()
    default: files.append(a)
    }
}

let printing = DispatchQueue(label: "out")
func say(_ line: String) {
    printing.sync {
        FileHandle.standardOutput.write((line + "\n").data(using: .utf8)!)
    }
}

/// The picture of `file` to `out` (a JPEG); nil, else why not
func draw(_ file: String, _ out: String, _ done: @escaping (String?) -> Void) {
    let req = QLThumbnailGenerator.Request(fileAt: URL(fileURLWithPath: file), size: CGSize(width: size, height: size),
                                           scale: 1, representationTypes: .thumbnail)
    QLThumbnailGenerator.shared.generateBestRepresentation(for: req) { rep, err in
        guard let rep = rep, rep.type == .thumbnail else { done(err.map { "\($0.localizedDescription)" } ?? "no thumbnail"); return }
        guard let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: out) as CFURL, UTType.jpeg.identifier as CFString, 1, nil) else {
            done("can't write \(out)"); return
        }
        CGImageDestinationAddImage(dest, rep.cgImage, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        done(CGImageDestinationFinalize(dest) ? nil : "can't write \(out)")
    }
}

let pending = DispatchGroup()
func oneLine(_ s: String) -> String { s.replacingOccurrences(of: "\n", with: " ").replacingOccurrences(of: "\t", with: " ") }

if let dir = outDir {
    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    for (n, f) in files.enumerated() {
        pending.enter()
        let t0 = DispatchTime.now()
        draw(f, "\(dir)/\(n).jpg") { why in
            let ms = Double(DispatchTime.now().uptimeNanoseconds - t0.uptimeNanoseconds) / 1e6
            say("\(f)\t\(Int(ms))\t\(why.map { "err " + oneLine($0) } ?? "ok")")
            pending.leave()
        }
    }
    pending.wait()
    exit(0)
}

// Requests from the Library, as long as it sends them
while let line = readLine(strippingNewline: true) {
    let parts = line.split(separator: "\t", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
    guard parts.count == 3 else { continue }
    let id = parts[0]
    pending.enter()
    draw(parts[1], parts[2]) { why in
        say("\(id)\t\(why.map { "err " + oneLine($0) } ?? "ok")")
        pending.leave()
    }
}
pending.wait()
