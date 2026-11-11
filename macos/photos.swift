// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-photos: what Action Footage Library reads of Apple Photos and of Apple's maps, on a Mac (docs/geolocation.md).
// It only reads: nothing here asks Photos for a change (no PHAssetChangeRequest, no performChanges), so the library is
// never touched. macOS has no read-only level for apps: the prompt it shows is its "Photos" one (readWrite is the only
// level that can read), attributed to the app that started this tool (Action Footage Library, whose Info.plist says why).
//
//   afp-photos status                     {"status": "notDetermined" | "denied" | "restricted" | "authorized" | "limited"}
//                                         as it is now, without asking
//   afp-photos request                    asks macOS (its prompt, once), then the status as above
//   afp-photos near <from> <to> ...       the photos and videos with a place, taken between each pair of unix times:
//                                         {"photos": [{"t", "lat", "lon", "alt", "acc", "id", "video"}]}, by time;
//                                         {"error": "not allowed", "status": …} without access (never asks)
//   afp-photos names <id>...              {"names": {id: "IMG_1234.HEIC"}}: the photos' own file names, to show
//   afp-photos place [--at <unix>] <lat> <lon> ...
//                                         Apple's reverse geocoding of each point, in English (one place a call to
//                                         Apple, a pause between: it limits how often an app may ask):
//                                         {"places": [{"lat", "lon", "name", "site", "locality", "sublocality", "region",
//                                         "subregion", "country", "cc", "ocean", "water", "tz", "tz_offset_s"} | {"error"}]}
//   afp-photos map <out.png> <w> <h> [--dark|--light] <lat> <lon> ...
//                                         a small map (Apple Maps, MKMapSnapshotter) around the points, each drawn as a
//                                         dot, written as a PNG at twice the size: {"ok": true} or {"error"}
//
// One JSON object on stdout; exit 0 even for "not allowed" (the Library reads the answer), 2 for a wrong call.

import Foundation
import Photos
import CoreLocation
import MapKit
import AppKit

func emit(_ obj: Any) {
    let data = (try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys])) ?? Data("{}".utf8)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
}

func statusName(_ s: PHAuthorizationStatus) -> String {
    switch s {
    case .notDetermined: return "notDetermined"
    case .restricted: return "restricted"
    case .denied: return "denied"
    case .authorized: return "authorized"
    case .limited: return "limited"
    @unknown default: return "unknown"
    }
}

func status() -> PHAuthorizationStatus { PHPhotoLibrary.authorizationStatus(for: .readWrite) }

func readable() -> Bool { let s = status(); return s == .authorized || s == .limited }

// Waits for a callback that may come on the main queue: the run loop keeps turning meanwhile
func wait(_ done: () -> Bool, seconds: Double) {
    let until = Date().addingTimeInterval(seconds)
    while !done() && Date() < until { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
}

var args = Array(CommandLine.arguments.dropFirst())
guard let cmd = args.first else {
    FileHandle.standardError.write(Data("usage: afp-photos status | request | near <from> <to>... | names <id>... | place <lat> <lon>...\n".utf8))
    exit(2)
}
args.removeFirst()

switch cmd {
case "status":
    emit(["status": statusName(status())])

case "request":
    if status() == .notDetermined {
        var answered = false
        PHPhotoLibrary.requestAuthorization(for: .readWrite) { _ in answered = true }
        wait({ answered }, seconds: 600)
    }
    emit(["status": statusName(status())])

case "near":
    guard readable() else { emit(["error": "not allowed", "status": statusName(status())]); exit(0) }
    let nums = args.compactMap(Double.init)
    guard nums.count >= 2, nums.count % 2 == 0 else { FileHandle.standardError.write(Data("near: pairs of unix times\n".utf8)); exit(2) }
    var out: [[String: Any]] = []
    var seen = Set<String>()
    for i in stride(from: 0, to: nums.count, by: 2) {
        let opts = PHFetchOptions()
        opts.predicate = NSPredicate(format: "creationDate >= %@ AND creationDate <= %@",
                                     Date(timeIntervalSince1970: nums[i]) as NSDate, Date(timeIntervalSince1970: nums[i + 1]) as NSDate)
        opts.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: true)]
        opts.includeAssetSourceTypes = [.typeUserLibrary, .typeCloudShared]
        let found = PHAsset.fetchAssets(with: opts)
        found.enumerateObjects { a, _, _ in
            guard let loc = a.location, let date = a.creationDate, !seen.contains(a.localIdentifier) else { return }
            let c = loc.coordinate
            guard CLLocationCoordinate2DIsValid(c), !(c.latitude == 0 && c.longitude == 0) else { return }
            seen.insert(a.localIdentifier)
            var p: [String: Any] = ["t": date.timeIntervalSince1970, "lat": c.latitude, "lon": c.longitude, "id": a.localIdentifier,
                                    "video": a.mediaType == .video]
            if loc.verticalAccuracy >= 0 { p["alt"] = loc.altitude }
            if loc.horizontalAccuracy >= 0 { p["acc"] = loc.horizontalAccuracy }
            out.append(p)
        }
    }
    out.sort { ($0["t"] as! Double) < ($1["t"] as! Double) }
    emit(["photos": out])

case "names":
    guard readable() else { emit(["error": "not allowed", "status": statusName(status())]); exit(0) }
    var names: [String: String] = [:]
    let found = PHAsset.fetchAssets(withLocalIdentifiers: args, options: nil)
    found.enumerateObjects { a, _, _ in
        if let r = PHAssetResource.assetResources(for: a).first { names[a.localIdentifier] = r.originalFilename }
    }
    emit(["names": names])

case "place":
    // --at <unix>: the time zone's offset at that time (summer time), else now
    var at = Date()
    if let i = args.firstIndex(of: "--at"), i + 1 < args.count, let t = Double(args[i + 1]) {
        at = Date(timeIntervalSince1970: t); args.removeSubrange(i...(i + 1))
    }
    let nums = args.compactMap(Double.init)
    guard nums.count >= 2, nums.count % 2 == 0 else { FileHandle.standardError.write(Data("place: pairs of lat lon\n".utf8)); exit(2) }
    let geocoder = CLGeocoder()
    var places: [[String: Any]] = []
    for i in stride(from: 0, to: nums.count, by: 2) {
        if i > 0 { Thread.sleep(forTimeInterval: 1.2) }
        let (lat, lon) = (nums[i], nums[i + 1])
        var result: [CLPlacemark]? = nil
        var failure: Error? = nil
        var done = false
        geocoder.reverseGeocodeLocation(CLLocation(latitude: lat, longitude: lon), preferredLocale: Locale(identifier: "en_US")) { r, e in
            result = r; failure = e; done = true
        }
        wait({ done }, seconds: 30)
        var p: [String: Any] = ["lat": lat, "lon": lon]
        if let pm = result?.first {
            p["name"] = pm.name
            p["site"] = pm.areasOfInterest?.first
            p["locality"] = pm.locality
            p["sublocality"] = pm.subLocality
            p["region"] = pm.administrativeArea
            p["subregion"] = pm.subAdministrativeArea
            p["country"] = pm.country
            p["cc"] = pm.isoCountryCode
            p["ocean"] = pm.ocean
            p["water"] = pm.inlandWater
            if let tz = pm.timeZone {
                p["tz"] = tz.identifier
                p["tz_offset_s"] = tz.secondsFromGMT(for: at)
            }
        } else if let e = failure as? CLError, e.code == .geocodeFoundNoResult {
            p["none"] = true
        } else {
            p["error"] = failure.map { "\($0.localizedDescription)" } ?? (done ? "no answer" : "timed out")
        }
        places.append(p)
    }
    emit(["places": places])

case "map":
    guard args.count >= 5, let w = Double(args[1]), let h = Double(args[2]) else {
        FileHandle.standardError.write(Data("map: <out.png> <w> <h> <lat> <lon>...\n".utf8)); exit(2)
    }
    let out = URL(fileURLWithPath: args[0])
    // --dark / --light: the page's look (the system's otherwise)
    let dark = args.contains("--dark") ? true : args.contains("--light") ? false : nil
    let nums = args.dropFirst(3).filter { !$0.hasPrefix("--") }.compactMap(Double.init)
    let pts = stride(from: 0, to: nums.count - 1, by: 2).map { CLLocationCoordinate2D(latitude: nums[$0], longitude: nums[$0 + 1]) }
    guard !pts.isEmpty else { emit(["error": "no points"]); exit(0) }
    // The points' box, padded, at least ~6 km across so one point shows where it is
    var (la0, la1, lo0, lo1) = (90.0, -90.0, 180.0, -180.0)
    for p in pts { la0 = min(la0, p.latitude); la1 = max(la1, p.latitude); lo0 = min(lo0, p.longitude); lo1 = max(lo1, p.longitude) }
    let span = MKCoordinateSpan(latitudeDelta: min(170, max(0.06, (la1 - la0) * 1.5)), longitudeDelta: min(350, max(0.06, (lo1 - lo0) * 1.5)))
    let opts = MKMapSnapshotter.Options()
    opts.region = MKCoordinateRegion(center: CLLocationCoordinate2D(latitude: (la0 + la1) / 2, longitude: (lo0 + lo1) / 2), span: span)
    opts.size = NSSize(width: w, height: h)
    opts.mapType = .mutedStandard
    if let dark = dark { opts.appearance = NSAppearance(named: dark ? .darkAqua : .aqua) }
    var snap: MKMapSnapshotter.Snapshot? = nil
    var failure: Error? = nil
    var done = false
    MKMapSnapshotter(options: opts).start(with: DispatchQueue.global()) { s, e in snap = s; failure = e; done = true }
    wait({ done }, seconds: 30)
    guard let snap = snap else { emit(["error": failure.map { "\($0.localizedDescription)" } ?? "timed out"]); exit(0) }
    // Drawn at twice the size, for a sharp picture on a Retina screen
    let scale = 2.0
    guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(w * scale), pixelsHigh: Int(h * scale), bitsPerSample: 8,
                                     samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
          let ctx = NSGraphicsContext(bitmapImageRep: rep) else { emit(["error": "no picture"]); exit(0) }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = ctx
    ctx.cgContext.scaleBy(x: scale, y: scale)
    snap.image.draw(in: NSRect(x: 0, y: 0, width: w, height: h))
    for p in pts {
        let at = snap.point(for: p)
        let r = 4.5
        // The snapshot's points are in its image's own (bottom-up) coordinates, as drawn here
        let dot = NSBezierPath(ovalIn: NSRect(x: at.x - r, y: at.y - r, width: 2 * r, height: 2 * r))
        NSColor(calibratedRed: 0.93, green: 0.27, blue: 0.2, alpha: 1).setFill(); dot.fill()
        NSColor.white.setStroke(); dot.lineWidth = 1.5; dot.stroke()
    }
    NSGraphicsContext.restoreGraphicsState()
    guard let png = rep.representation(using: .png, properties: [:]), (try? png.write(to: out)) != nil else { emit(["error": "couldn't write"]); exit(0) }
    emit(["ok": true])

default:
    FileHandle.standardError.write(Data("afp-photos: unknown command \(cmd)\n".utf8))
    exit(2)
}
