// sdr-white: how bright macOS takes each external SDR monitor's white to be, read and set.
//
// macOS (Apple Silicon) places HDR video on a screen by its SDR white in nits: an HDR pixel of n nits shows at n / white
// of the screen's white, then what's above white is compressed into it. For an external monitor it doesn't know the
// brightness of, it makes one up: 200 nits on the first external display controller (dispext0), nothing (1.0) on the
// second. On an SDR monitor, which has no room above white, 200 puts HDR's mid-tones a stop lower than BT.2408 / NBCU
// do; 100 shows HLG and Dolby Vision as NBCU's conversion does. The value is the IOMobileFramebuffer's
// IOMFBBrightnessLevel (16.16 fixed point nits), writable without root. Nothing is saved by macOS: plugging the
// monitor in again, or restarting, gives its own value back.
//
// Only external screens without HDR headroom are touched (NSScreen's potential EDR 1: SDR monitors, and HDR ones that
// macOS has in SDR); never the built-in display, nor one macOS shows HDR on.
//
// Usage:
//   sdr-white                  the screens and the light sensor, as JSON
//   sdr-white set <nits>       every external SDR screen's white to <nits>
//   sdr-white restore          back to macOS's own values, where they're still the ones set here
// The value each screen had before it was first set here is kept in <state> (--state <path>; default
// ~/Library/Application Support/ActionFootagePlayer/sdr-white.json), so restore works after a restart too.

import AppKit
import CoreGraphics
import Foundation
import IOKit

let KEY = "IOMFBBrightnessLevel" as CFString

struct Screen {
    let service: io_service_t
    let dcp: String          // dispext0, dispext1, ...
    let name: String
    let key: String          // vendor-product-serial: how a monitor is told apart
    let level: Double        // nits
    let sdr: Bool
}

func fixed(_ v: CFTypeRef?) -> Double? {
    guard let n = v as? NSNumber else { return nil }
    return n.doubleValue / 65536
}

// The online displays by vendor-product-serial, with their name and whether macOS shows HDR on them
func displays() -> [String: (name: String, sdr: Bool)] {
    var out: [String: (String, Bool)] = [:]
    var ids = [CGDirectDisplayID](repeating: 0, count: 16)
    var n: UInt32 = 0
    CGGetOnlineDisplayList(16, &ids, &n)
    for id in ids.prefix(Int(n)) {
        let screen = NSScreen.screens.first {
            ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == id
        }
        let edr = screen?.maximumPotentialExtendedDynamicRangeColorComponentValue ?? 1
        let key = "\(CGDisplayVendorNumber(id))-\(CGDisplayModelNumber(id))-\(CGDisplaySerialNumber(id))"
        out[key] = (screen?.localizedName ?? "Display \(id)", edr <= 1)
    }
    return out
}

func screens() -> [Screen] {
    var list: [Screen] = []
    let online = displays()
    var it: io_iterator_t = 0
    guard IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("IOMobileFramebuffer"), &it) == KERN_SUCCESS else {
        return list
    }
    while true {
        let s = IOIteratorNext(it)
        if s == 0 { break }
        var p = [CChar](repeating: 0, count: 1024)
        IORegistryEntryGetPath(s, kIOServicePlane, &p)
        let path = String(cString: p)
        guard let r = path.range(of: #"/dispext\d+@"#, options: .regularExpression),
              let level = fixed(IORegistryEntryCreateCFProperty(s, KEY, nil, 0)?.takeRetainedValue()),
              let attrs = IORegistryEntryCreateCFProperty(s, "DisplayAttributes" as CFString, nil, 0)?.takeRetainedValue() as? [String: Any],
              let product = attrs["ProductAttributes"] as? [String: Any] else {
            IOObjectRelease(s)
            continue
        }
        let vendor = (product["LegacyManufacturerID"] as? NSNumber)?.uint32Value ?? 0
        let model = (product["ProductID"] as? NSNumber)?.uint32Value ?? 0
        let serial = (product["SerialNumber"] as? NSNumber)?.uint32Value ?? 0
        let key = "\(vendor)-\(model)-\(serial)"
        guard let d = online[key] else {
            IOObjectRelease(s)
            continue
        }
        let dcp = String(path[r]).trimmingCharacters(in: CharacterSet(charactersIn: "/@"))
        list.append(Screen(service: s, dcp: dcp, name: (product["ProductName"] as? String) ?? d.name, key: key,
                           level: level, sdr: d.sdr))
    }
    IOObjectRelease(it)
    return list.sorted { $0.dcp < $1.dcp }
}

// The light sensor of a MacBook (lux), when its lid is open
func lux() -> Double? {
    let root = IORegistryGetRootEntry(kIOMainPortDefault)
    defer { IOObjectRelease(root) }
    let v = IORegistryEntrySearchCFProperty(root, kIOServicePlane, "CurrentLux" as CFString, nil,
                                            IOOptionBits(kIORegistryIterateRecursively))
    // -1 while it has no reading (the lid closing, or just woken)
    guard let l = (v as? NSNumber)?.doubleValue, l >= 0, l < 200_000 else { return nil }
    return l
}

func write(_ s: Screen, _ nits: Double) -> Bool {
    let v = NSNumber(value: Int64((nits * 65536).rounded()))
    return IORegistryEntrySetCFProperty(s.service, KEY, v) == KERN_SUCCESS
        && abs((fixed(IORegistryEntryCreateCFProperty(s.service, KEY, nil, 0)?.takeRetainedValue()) ?? -1) - nits) < 0.01
}

// What was there before this set a screen (original), and what it set (written), by monitor
var args = Array(CommandLine.arguments.dropFirst())
var statePath = NSString(string: "~/Library/Application Support/ActionFootagePlayer/sdr-white.json").expandingTildeInPath
if let i = args.firstIndex(of: "--state"), i + 1 < args.count {
    statePath = args[i + 1]
    args.removeSubrange(i...i + 1)
}
func loadState() -> [String: [String: Double]] {
    guard let d = FileManager.default.contents(atPath: statePath),
          let j = try? JSONSerialization.jsonObject(with: d) as? [String: [String: Double]] else { return [:] }
    return j
}
func saveState(_ st: [String: [String: Double]]) {
    try? FileManager.default.createDirectory(atPath: (statePath as NSString).deletingLastPathComponent,
                                             withIntermediateDirectories: true)
    if let d = try? JSONSerialization.data(withJSONObject: st, options: [.prettyPrinted, .sortedKeys]) {
        FileManager.default.createFile(atPath: statePath, contents: d)
    }
}

func report(_ list: [Screen], _ results: [String: Bool] = [:]) {
    var out: [String: Any] = [:]
    out["lux"] = lux() ?? NSNull()
    let st = loadState()
    out["screens"] = list.map { s -> [String: Any] in
        var d: [String: Any] = ["name": s.name, "dcp": s.dcp, "white": (s.level * 10).rounded() / 10, "sdr": s.sdr]
        if let o = st[s.key]?["original"] { d["original"] = o }
        if let r = results[s.key] { d["ok"] = r }
        return d
    }
    let data = try! JSONSerialization.data(withJSONObject: out, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
}

let list = screens()
switch args.first {
case nil, "list":
    report(list)
case "set":
    guard args.count >= 2, let nits = Double(args[1]), nits >= 20, nits <= 1000 else {
        FileHandle.standardError.write("sdr-white set <nits>, 20 to 1000\n".data(using: .utf8)!)
        exit(2)
    }
    var st = loadState()
    var results: [String: Bool] = [:]
    for s in list where s.sdr {
        // macOS's own value: the one there unless it's what this wrote last (else macOS has set it again since)
        var e = st[s.key] ?? [:]
        if e["written"].map({ abs($0 - s.level) >= 0.01 }) ?? true { e["original"] = s.level }
        let ok = write(s, nits)
        if ok { e["written"] = nits }
        st[s.key] = e
        results[s.key] = ok
    }
    saveState(st)
    report(screens(), results)
case "restore":
    var st = loadState()
    var results: [String: Bool] = [:]
    for s in list {
        guard let e = st[s.key], let o = e["original"] else { continue }
        if let w = e["written"], abs(w - s.level) < 0.01 { results[s.key] = write(s, o) }
        st[s.key] = nil
    }
    saveState(st)
    report(screens(), results)
default:
    FileHandle.standardError.write("usage: sdr-white [list | set <nits> | restore] [--state <path>]\n".data(using: .utf8)!)
    exit(2)
}
for s in list { IOObjectRelease(s.service) }
