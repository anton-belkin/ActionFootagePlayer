// Folders and clips: the sources (Files tab), a folder's subfolders and videos with their first frames, copying a
// clip onto the phone.
import SwiftUI
import AVFoundation
import UniformTypeIdentifiers

struct SourcesView: View {
    @EnvironmentObject var library: Library
    @State private var picking = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    NavigationLink { FolderView(url: Library.documents, title: "On this iPhone") } label: {
                        Label("On this iPhone", systemImage: "iphone")
                    }
                }
                Section {
                    ForEach(library.sources) { s in
                        if let url = library.url(of: s) {
                            NavigationLink { FolderView(url: url, title: s.name) } label: {
                                Label(s.name, systemImage: "sdcard")
                            }
                        } else {
                            Label("\(s.name) — not connected", systemImage: "sdcard").foregroundStyle(.secondary)
                        }
                    }
                    .onDelete { idx in idx.map { library.sources[$0] }.forEach(library.remove) }
                    Button { picking = true } label: { Label("Add a folder…", systemImage: "plus") }
                } header: {
                    Text("Folders")
                } footer: {
                    Text("An SD card in a reader, a camera seen as a disk, or the NAS's share (connect to it in Files first: Browse → ⋯ → Connect to Server, smb://…). Pick its top folder or DCIM.")
                }
                if let error { Text(error).foregroundStyle(.red).font(.footnote) }
            }
            .navigationTitle("Footage")
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                switch result {
                case .success(let url):
                    do { try library.add(url) } catch { self.error = error.localizedDescription }
                case .failure(let e): error = e.localizedDescription
                }
            }
        }
    }
}

struct FolderItem: Identifiable, Hashable {
    var id: URL { url }
    let url: URL
    let isFolder: Bool
    let size: Int64
    let date: Date?
}

func isVideo(_ url: URL) -> Bool {
    let ext = url.pathExtension.lowercased()
    return (ext == "mp4" || ext == "mov") && !url.lastPathComponent.lowercased().hasSuffix(".afplink.mp4")
}

struct FolderView: View {
    @EnvironmentObject var library: Library
    let url: URL
    let title: String
    @State private var items: [FolderItem] = []
    @State private var loading = true
    @State private var status: String?

    var body: some View {
        List {
            if let status { Text(status).font(.footnote).foregroundStyle(.secondary) }
            ForEach(items) { item in
                if item.isFolder {
                    NavigationLink { FolderView(url: item.url, title: item.url.lastPathComponent) } label: {
                        Label(item.url.lastPathComponent, systemImage: "folder")
                    }
                } else {
                    NavigationLink { PlayerScreen(item: PlayItem(url: item.url, title: item.url.lastPathComponent)) } label: {
                        ClipRow(item: item)
                    }
                    .contextMenu {
                        if !item.url.path.hasPrefix(Library.documents.path) {
                            Button { copyToPhone(item.url) } label: { Label("Copy to this iPhone", systemImage: "square.and.arrow.down") }
                        }
                    }
                }
            }
            if !loading && items.isEmpty { Text("No folders or videos here").foregroundStyle(.secondary) }
        }
        .overlay { if loading { ProgressView() } }
        .navigationTitle(title)
        .task { await load() }
        .refreshable { await load() }
    }

    func load() async {
        let url = self.url
        let list: [FolderItem] = await Task.detached {
            let keys: [URLResourceKey] = [.isDirectoryKey, .fileSizeKey, .contentModificationDateKey]
            let urls = (try? FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles])) ?? []
            return urls.compactMap { u -> FolderItem? in
                let v = try? u.resourceValues(forKeys: Set(keys))
                let folder = v?.isDirectory ?? false
                if !folder && !isVideo(u) { return nil }
                return FolderItem(url: u, isFolder: folder, size: Int64(v?.fileSize ?? 0), date: v?.contentModificationDate)
            }.sorted { a, b in a.isFolder != b.isFolder ? a.isFolder : a.url.lastPathComponent.localizedStandardCompare(b.url.lastPathComponent) == .orderedAscending }
        }.value
        items = list
        loading = false
    }

    /// The clip and what belongs to it (its own file, its Gyroflow project) into the app's folder, under the folder's name
    func copyToPhone(_ clip: URL) {
        let dest = Library.documents.appendingPathComponent(url.lastPathComponent, isDirectory: true)
        status = "Copying \(clip.lastPathComponent)…"
        Task.detached {
            let fm = FileManager.default
            let msg: String
            do {
                try fm.createDirectory(at: dest, withIntermediateDirectories: true)
                let base = clip.deletingPathExtension()
                for f in [clip, base.appendingPathExtension("afp.json"), base.appendingPathExtension("gyroflow"),
                          URL(fileURLWithPath: clip.path + ".afp.json")] where fm.fileExists(atPath: f.path) {
                    let to = dest.appendingPathComponent(f.lastPathComponent)
                    if fm.fileExists(atPath: to.path) { try fm.removeItem(at: to) }
                    try fm.copyItem(at: f, to: to)
                }
                msg = "Copied \(clip.lastPathComponent) to On this iPhone/\(dest.lastPathComponent)"
            } catch { msg = "Copy failed: \(error.localizedDescription)" }
            await MainActor.run { status = msg }
        }
    }
}

struct ClipRow: View {
    let item: FolderItem
    var body: some View {
        HStack(spacing: 12) {
            Thumbnail(url: item.url).frame(width: 96, height: 54).clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.url.lastPathComponent).font(.subheadline).lineLimit(1)
                Text([ByteCountFormatter.string(fromByteCount: item.size, countStyle: .file),
                      item.date.map { $0.formatted(date: .abbreviated, time: .shortened) } ?? ""].filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

final class ThumbCache {
    static let shared = NSCache<NSURL, UIImage>()
}

struct Thumbnail: View {
    let url: URL
    @State private var image: UIImage?
    var body: some View {
        ZStack {
            Color.gray.opacity(0.2)
            if let image { Image(uiImage: image).resizable().scaledToFill() }
        }
        .task(id: url) {
            if let i = ThumbCache.shared.object(forKey: url as NSURL) { image = i; return }
            let gen = AVAssetImageGenerator(asset: AVURLAsset(url: url))
            gen.appliesPreferredTrackTransform = true
            gen.maximumSize = CGSize(width: 320, height: 180)
            if let cg = try? await gen.image(at: CMTime(seconds: 1, preferredTimescale: 600)).image {
                let i = UIImage(cgImage: cg)
                ThumbCache.shared.setObject(i, forKey: url as NSURL)
                image = i
            }
        }
    }
}
