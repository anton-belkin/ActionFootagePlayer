// Where footage is: folders picked in Files (an SD card in a reader, a camera seen as a disk, the NAS's SMB share
// connected in Files), kept as bookmarks, plus the app's own folder ("On this iPhone", copies). And the player's
// settings: the afp-stream address, the interpolation.
import Foundation
import SwiftUI

struct Source: Identifiable, Codable, Hashable {
    var id = UUID()
    var name: String
    var bookmark: Data
}

@MainActor
final class Library: ObservableObject {
    @Published var sources: [Source] = []
    @Published var server: String { didSet { UserDefaults.standard.set(server, forKey: "server") } }
    @Published var interpolation: String { didSet { UserDefaults.standard.set(interpolation, forKey: "interpolation") } }
    /// Folders being read now (security-scoped access started once, kept while the app runs)
    private var open: [UUID: URL] = [:]

    static let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]

    init() {
        server = UserDefaults.standard.string(forKey: "server") ?? "http://192.168.50.186:8099"
        interpolation = UserDefaults.standard.string(forKey: "interpolation") ?? "bicubic9"
        if let data = UserDefaults.standard.data(forKey: "sources"),
           let list = try? JSONDecoder().decode([Source].self, from: data) { sources = list }
    }

    private func save() {
        if let data = try? JSONEncoder().encode(sources) { UserDefaults.standard.set(data, forKey: "sources") }
    }

    func add(_ url: URL) throws {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let data = try url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
        sources.append(Source(name: url.lastPathComponent, bookmark: data))
        save()
    }

    func remove(_ s: Source) {
        if let url = open.removeValue(forKey: s.id) { url.stopAccessingSecurityScopedResource() }
        sources.removeAll { $0.id == s.id }
        save()
    }

    /// The source's folder, its access started; nil when it isn't there (the card not plugged in, the share away)
    func url(of s: Source) -> URL? {
        if let u = open[s.id] { return u }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: s.bookmark, options: [], relativeTo: nil, bookmarkDataIsStale: &stale) else { return nil }
        _ = url.startAccessingSecurityScopedResource()
        if stale, let data = try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil),
           let i = sources.firstIndex(where: { $0.id == s.id }) {
            sources[i].bookmark = data
            save()
        }
        open[s.id] = url
        return url
    }

    /// A clip of the NAS's catalog in one of the folders here: the project's folder (its last component) or the
    /// clip's path under a source, so the phone can play the original itself
    func findOriginal(project: String, clip: String) -> URL? {
        let projectName = (project as NSString).lastPathComponent
        let roots = [Library.documents] + sources.compactMap { url(of: $0) }
        let fm = FileManager.default
        for root in roots {
            for candidate in [root.appendingPathComponent(clip),
                              root.appendingPathComponent(projectName).appendingPathComponent(clip),
                              root.appendingPathComponent((clip as NSString).lastPathComponent)] {
                if fm.fileExists(atPath: candidate.path) { return candidate }
            }
        }
        return nil
    }

    /// The afp-stream address with no slash at the end
    var serverBase: String { server.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "/")) }

    /// A URL the NAS gave (its own address in it) made to go to the address set here (Tailscale, another name)
    func rebase(_ s: String) -> URL? {
        guard let u = URL(string: s), var c = URLComponents(string: serverBase) else { return URL(string: s) }
        c.path = u.path
        c.query = u.query
        return c.url
    }
}
