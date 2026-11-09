// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-quicklook <file>...: the system's Quick Look panel for these files, as Finder's space bar shows it, for Action
// Footage Library (its space bar). It quits when the panel closes; the Library ends it to close the panel. qlmanage -p,
// Quick Look's debugging tool, crashes on videos on macOS 27 (AVKit's highlighted time ranges); the panel doesn't.

import AppKit
import Quartz

final class Looker: NSObject, NSApplicationDelegate, QLPreviewPanelDataSource, QLPreviewPanelDelegate {
    let urls: [URL]
    init(_ urls: [URL]) { self.urls = urls }

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.activate(ignoringOtherApps: true)
        guard let panel = QLPreviewPanel.shared() else { NSApp.terminate(nil); return }
        panel.dataSource = self
        panel.delegate = self
        panel.makeKeyAndOrderFront(nil)
        panel.reloadData()
        NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: panel, queue: .main) { _ in
            NSApp.terminate(nil)
        }
    }

    func numberOfPreviewItems(in panel: QLPreviewPanel!) -> Int { urls.count }
    func previewPanel(_ panel: QLPreviewPanel!, previewItemAt index: Int) -> QLPreviewItem! { urls[index] as NSURL }

    // The space bar closes it, as in Finder
    func previewPanel(_ panel: QLPreviewPanel!, handle event: NSEvent!) -> Bool {
        if event.type == .keyDown, event.charactersIgnoringModifiers == " " { panel.close(); return true }
        return false
    }
}

let files = CommandLine.arguments.dropFirst().map { URL(fileURLWithPath: $0) }
if files.isEmpty { FileHandle.standardError.write("usage: afp-quicklook <file>...\n".data(using: .utf8)!); exit(2) }
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let looker = Looker(files)
app.delegate = looker
app.run()
