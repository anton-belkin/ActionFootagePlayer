// The iPhone prototype of Action Footage Player (docs/mobile-app.md): folders on a card, a disk or the NAS's share,
// played stabilized and with the log curve converted on the phone's GPU; the NAS's library (afp-stream) as streams.
import SwiftUI

@main
struct AFPApp: App {
    @StateObject private var library = Library()

    var body: some Scene {
        WindowGroup {
            TabView {
                SourcesView()
                    .tabItem { Label("Files", systemImage: "folder") }
                CatalogView()
                    .tabItem { Label("NAS", systemImage: "externaldrive.connected.to.line.below") }
                SettingsView()
                    .tabItem { Label("Settings", systemImage: "gearshape") }
            }
            .environmentObject(library)
            .preferredColorScheme(.dark)
        }
    }
}
