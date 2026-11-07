// tools/stream/hlsplay.swift: swiftc -O hlsplay.swift -o hlsplay; ./hlsplay <url> [seek seconds] [2-second rounds]
// Plays an HLS URL with AVFoundation (what Safari and iOS use) for a few seconds and reports status, errors, logs
import AVFoundation
import Foundation
let url = URL(string: CommandLine.arguments[1])!
let item = AVPlayerItem(url: url)
let player = AVPlayer(playerItem: item)
player.isMuted = true
player.play()
let start = Date()
var reported = false
func dump() {
    print(String(format: "t=%.1f status=%d time=%.2f rate=%.1f likely=%@ stall=%@", Date().timeIntervalSince(start), item.status.rawValue,
                 item.currentTime().seconds, player.rate, item.isPlaybackLikelyToKeepUp ? "y" : "n", player.reasonForWaitingToPlay?.rawValue ?? "-"))
    if let e = item.error as NSError? { print("ERROR:", e.domain, e.code, e.localizedDescription, e.userInfo) }
    if let log = item.errorLog() { for ev in log.events { print("errorlog:", ev.errorStatusCode, ev.errorDomain, ev.errorComment ?? "", ev.uri ?? "") } }
    if let a = item.accessLog(), let ev = a.events.last { print("access: segments \(ev.numberOfMediaRequests) indicated \(ev.indicatedBitrate) observed \(ev.observedBitrate) dropped \(ev.numberOfDroppedVideoFrames)") }
    for t in item.tracks { if let a = t.assetTrack { print("track:", a.mediaType.rawValue, (a.formatDescriptions as! [CMFormatDescription]).map { CMFormatDescriptionGetMediaSubType($0) }.map { String(format: "%c%c%c%c", ($0 >> 24) & 255, ($0 >> 16) & 255, ($0 >> 8) & 255, $0 & 255) }) } }
}
let seekTo = CommandLine.arguments.count > 2 ? Double(CommandLine.arguments[2]) : nil
let rounds = CommandLine.arguments.count > 3 ? Int(CommandLine.arguments[3])! : 12
for r in 0..<rounds {
    RunLoop.main.run(until: Date().addingTimeInterval(2)); dump()
    if item.status == .failed { break }
    if r == 3, let s = seekTo { print("seek to", s); player.seek(to: CMTime(seconds: s, preferredTimescale: 600)) }
}
