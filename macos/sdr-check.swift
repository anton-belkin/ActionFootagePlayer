// sdr-check: whether macOS has an external SDR monitor set up as an HDR display, which shows HDR on it darker.
//
// On Apple Silicon each external display controller (dispext0, dispext1, ...) has an IOMobileFramebuffer. An SDR
// monitor normally has no backlight cap there (BLNitsCap) and its brightness level unset (IOMFBBrightnessLevel 1.0).
// Once an HDR display has been on a controller (an HDR TV, say), the controller can keep the cap and a level
// (1000 / 200 nits seen) after an SDR monitor takes its place, through unplugging and plugging, until the Mac restarts;
// macOS then shows HDR video on that monitor (QuickTime, Safari, the player's hdr and dv looks) about a stop darker in
// the mid-tones. Writing the values from user space doesn't change that: restarting does. This only reads.
//
// Prints the external screens as JSON: {"screens": [{"name", "dcp", "sdr", "white", "cap", "stale"}]}; stale: an SDR
// screen (no HDR headroom) whose controller has a cap or a brightness level set.

import AppKit
import CoreGraphics
import Foundation
import IOKit

func nits(_ v: CFTypeRef?) -> Double? {
    guard let n = v as? NSNumber else { return nil }
    return n.doubleValue / 65536
}

// The online displays by vendor-product-serial, with their name and whether macOS shows HDR on them
var online: [String: (name: String, sdr: Bool)] = [:]
var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
for id in ids.prefix(Int(count)) {
    let screen = NSScreen.screens.first {
        ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == id
    }
    let edr = screen?.maximumPotentialExtendedDynamicRangeColorComponentValue ?? 1
    online["\(CGDisplayVendorNumber(id))-\(CGDisplayModelNumber(id))-\(CGDisplaySerialNumber(id))"] =
        (screen?.localizedName ?? "Display \(id)", edr <= 1)
}

var screens: [[String: Any]] = []
var it: io_iterator_t = 0
if IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("IOMobileFramebuffer"), &it) == KERN_SUCCESS {
    while true {
        let s = IOIteratorNext(it)
        if s == 0 { break }
        defer { IOObjectRelease(s) }
        var p = [CChar](repeating: 0, count: 1024)
        IORegistryEntryGetPath(s, kIOServicePlane, &p)
        let path = String(cString: p)
        guard let r = path.range(of: #"/dispext\d+@"#, options: .regularExpression),
              let attrs = IORegistryEntryCreateCFProperty(s, "DisplayAttributes" as CFString, nil, 0)?.takeRetainedValue() as? [String: Any],
              let product = attrs["ProductAttributes"] as? [String: Any] else { continue }
        let key = ["LegacyManufacturerID", "ProductID", "SerialNumber"]
            .map { String((product[$0] as? NSNumber)?.uint32Value ?? 0) }.joined(separator: "-")
        guard let d = online[key] else { continue }
        let white = nits(IORegistryEntryCreateCFProperty(s, "IOMFBBrightnessLevel" as CFString, nil, 0)?.takeRetainedValue())
        let cap = nits(IORegistryEntryCreateCFProperty(s, "BLNitsCap" as CFString, nil, 0)?.takeRetainedValue())
        let set = cap != nil || (white.map { abs($0 - 1) > 0.01 } ?? false)
        var e: [String: Any] = ["name": (product["ProductName"] as? String) ?? d.name,
                                "dcp": String(path[r]).trimmingCharacters(in: CharacterSet(charactersIn: "/@")),
                                "sdr": d.sdr, "stale": d.sdr && set]
        if let w = white { e["white"] = (w * 10).rounded() / 10 }
        if let c = cap { e["cap"] = (c * 10).rounded() / 10 }
        screens.append(e)
    }
    IOObjectRelease(it)
}
let data = try! JSONSerialization.data(withJSONObject: ["screens": screens], options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
