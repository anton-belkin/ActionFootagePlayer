// The NAS's library through afp-stream (tools/stream): its catalog of moments and clip previews by project and
// folder, each as a stream the NAS renders, or played on the phone from the original when a folder here has it.
import SwiftUI
import AVKit

struct CatalogItem: Decodable, Identifiable, Hashable {
    let id: String
    let kind: String
    let project: String
    let folder: String?
    let name: String
    let title: String?
    let clip: String?
    let start_in: Double?
    let duration: Double?
    let start: String?
    let stabilized: Bool?
    let stream: String
    let poster: String?

    enum CodingKeys: String, CodingKey {
        case id, kind, project, folder, name, title, clip, duration, start, stabilized, stream, poster
        case start_in = "in"
    }
}

struct Catalog: Decodable { let items: [CatalogItem] }

struct CatalogView: View {
    @EnvironmentObject var library: Library
    @State private var items: [CatalogItem] = []
    @State private var error: String?
    @State private var loading = false
    @State private var kind = "moment"

    var groups: [(String, [CatalogItem])] {
        let list = items.filter { $0.kind == kind }
        let byFolder = Dictionary(grouping: list) { $0.folder?.isEmpty == false ? $0.folder! : (($0.project as NSString).lastPathComponent) }
        return byFolder.keys.sorted().map { ($0, byFolder[$0]!.sorted { ($0.start ?? $0.name) < ($1.start ?? $1.name) }) }
    }

    var body: some View {
        NavigationStack {
            List {
                Picker("Show", selection: $kind) {
                    Text("Moments").tag("moment")
                    Text("Clips").tag("preview")
                }
                .pickerStyle(.segmented)
                if let error { Text(error).foregroundStyle(.red).font(.footnote) }
                ForEach(groups, id: \.0) { folder, list in
                    Section(folder) {
                        ForEach(list) { item in
                            NavigationLink { CatalogDetail(item: item) } label: { CatalogRow(item: item) }
                        }
                    }
                }
            }
            .overlay { if loading && items.isEmpty { ProgressView("Asking \(library.serverBase)…") } }
            .navigationTitle("NAS")
            .task { await load() }
            .refreshable { await load() }
        }
    }

    func load() async {
        loading = true
        defer { loading = false }
        guard let url = URL(string: library.serverBase + "/catalog.json") else { error = "Bad address: \(library.server)"; return }
        do {
            var req = URLRequest(url: url)
            req.timeoutInterval = 20
            let (data, _) = try await URLSession.shared.data(for: req)
            items = try JSONDecoder().decode(Catalog.self, from: data).items
            error = nil
        } catch {
            self.error = "\(url.absoluteString): \(error.localizedDescription)"
        }
    }
}

func durationText(_ s: Double?) -> String {
    guard let s, s.isFinite else { return "" }
    let t = Int(s.rounded())
    return t >= 3600 ? String(format: "%d:%02d:%02d", t / 3600, t / 60 % 60, t % 60) : String(format: "%d:%02d", t / 60, t % 60)
}

struct CatalogRow: View {
    @EnvironmentObject var library: Library
    let item: CatalogItem
    var body: some View {
        HStack(spacing: 12) {
            AsyncImage(url: item.poster.flatMap(library.rebase)) { img in img.resizable().scaledToFill() } placeholder: { Color.gray.opacity(0.2) }
                .frame(width: 96, height: 54).clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title ?? item.name).font(.subheadline).lineLimit(2)
                Text([durationText(item.duration), item.start?.replacingOccurrences(of: "T", with: " ") ?? ""].filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

struct CatalogDetail: View {
    @EnvironmentObject var library: Library
    let item: CatalogItem
    @State private var original: URL?
    @State private var looked = false

    var body: some View {
        List {
            Section {
                AsyncImage(url: item.poster.flatMap(library.rebase)) { img in img.resizable().scaledToFit() } placeholder: { Color.gray.opacity(0.2).aspectRatio(16/9, contentMode: .fit) }
                    .listRowInsets(EdgeInsets())
            }
            Section {
                if let url = library.rebase(item.stream) {
                    NavigationLink { StreamPlayer(url: url).navigationTitle(item.title ?? item.name).navigationBarTitleDisplayMode(.inline) } label: {
                        Label("Stream: the NAS renders it", systemImage: "antenna.radiowaves.left.and.right")
                    }
                }
                if let original {
                    NavigationLink {
                        PlayerScreen(item: PlayItem(url: original, title: item.title ?? item.name,
                                                    start: item.start_in ?? 0, duration: item.kind == "moment" ? item.duration : nil))
                    } label: {
                        Label("On this iPhone: stabilized here", systemImage: "iphone.gen3")
                    }
                } else if looked {
                    Label("On this iPhone: the original isn't in your folders (add the NAS's share in Files)", systemImage: "iphone.slash")
                        .foregroundStyle(.secondary).font(.footnote)
                }
            } footer: {
                Text([item.clip ?? "", item.stabilized == true ? "stabilized" : "not stabilized"].filter { !$0.isEmpty }.joined(separator: " · "))
            }
        }
        .navigationTitle(item.title ?? item.name)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            if let clip = item.clip {
                let project = item.project
                original = library.findOriginal(project: project, clip: clip)
            }
            looked = true
        }
    }
}

/// An afp-stream HLS stream (HEVC, HLG): AVKit's own player shows it in HDR
struct StreamPlayer: View {
    let url: URL
    @State private var player: AVPlayer?
    var body: some View {
        VideoPlayer(player: player)
            .ignoresSafeArea(edges: .bottom)
            .toolbar(.hidden, for: .tabBar)
            .onAppear { let p = AVPlayer(url: url); player = p; p.play() }
            .onDisappear { player?.pause() }
    }
}

struct SettingsView: View {
    @EnvironmentObject var library: Library
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("http://nas:8099", text: $library.server)
                        .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                } header: { Text("afp-stream on the NAS") } footer: {
                    Text("At home its address on your network; away, its Tailscale address or name.")
                }
                Section {
                    Picker("Interpolation", selection: $library.interpolation) {
                        Text("Bicubic (9 reads)").tag("bicubic9")
                        Text("Bicubic (16 reads)").tag("bicubic")
                        Text("Lanczos").tag("lanczos")
                        Text("Bilinear (fastest)").tag("bilinear")
                    }
                } header: { Text("Stabilized on this iPhone") }
                Section {
                    Text("Prototype \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")")
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Settings")
        }
    }
}
