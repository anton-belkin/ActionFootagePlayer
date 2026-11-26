// AFP Runner: the GitHub Actions runner on Anton's Mac (anton-mac, ~/actions-runner) in the menu bar. Its icon says
// whether the runner is stopped, idle or working; its panel shows the Mac's CPU and memory, the job the runner runs,
// what's waiting on GitHub, and Stop / Start.
//
// Stop takes the runner's LaunchAgent out of launchd and disables it (it stays stopped after a restart or a new login);
// Start enables and loads it again. A job running when Stop is pressed: stop now (the job fails; Start reruns it) or
// after this job (a detached shell waits for it, so quitting this app doesn't cancel that).
//
// What it decides is written to ~/actions-runner/control.json, for Claude Code sessions and scripts to read:
//   {"state": "running" | "stopping" | "stopped", "by": "Anton (AFP Runner)", "at": "<ISO time>", "reason": "...",
//    "interrupted_runs": [<run ids Stop now cut short, rerun by Start>]}
// "stopped" there means Anton stopped it on purpose: jobs wait on GitHub until he presses Start; nobody else starts it.
//
// Build and install: tools/ci-runner/make_runner_bar.sh (swiftc, no Xcode project). Needs the gh CLI logged in.
import AppKit
import Darwin
import Foundation
import ServiceManagement
import SwiftUI

let REPO = "anton-belkin/ActionFootagePlayer"
let RUNNER_NAME = "anton-mac"
let LABEL = "actions.runner.anton-belkin-ActionFootagePlayer.anton-mac"
let HOME = FileManager.default.homeDirectoryForCurrentUser.path
let RUNNER_PLIST = "\(HOME)/Library/LaunchAgents/\(LABEL).plist"
let CONTROL = "\(HOME)/actions-runner/control.json"
let LOG = "\(HOME)/Library/Logs/afp-runner-bar.log"
let DOMAIN = "gui/\(getuid())"

// ---- Running commands ----

@discardableResult
func run(_ args: [String], timeout: TimeInterval = 30) -> (status: Int32, out: String) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: args[0])
    p.arguments = Array(args.dropFirst())
    var env = ProcessInfo.processInfo.environment
    env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    p.environment = env
    let pipe = Pipe()
    p.standardOutput = pipe
    p.standardError = Pipe()
    do { try p.run() } catch { return (-1, "") }
    let deadline = Date().addingTimeInterval(timeout)
    var data = Data()
    let reader = DispatchQueue(label: "read")
    let done = DispatchSemaphore(value: 0)
    reader.async { data = pipe.fileHandleForReading.readDataToEndOfFile(); done.signal() }
    while p.isRunning && Date() < deadline { usleep(20_000) }
    if p.isRunning { p.terminate() }
    _ = done.wait(timeout: .now() + 2)
    p.waitUntilExit()
    return (p.terminationStatus, String(data: data, encoding: .utf8) ?? "")
}

let GH: String = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"].first { FileManager.default.isExecutableFile(atPath: $0) } ?? "gh"

func log(_ s: String) {
    let line = "\(ISO8601DateFormatter().string(from: Date())) \(s)\n"
    if let h = FileHandle(forWritingAtPath: LOG) { h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close() }
    else { try? line.write(toFile: LOG, atomically: true, encoding: .utf8) }
}

// ---- The control file ----

struct Control: Codable {
    var state: String
    var by: String
    var at: String
    var reason: String
    var interrupted_runs: [Int]
}

func readControl() -> Control? {
    guard let d = FileManager.default.contents(atPath: CONTROL) else { return nil }
    return try? JSONDecoder().decode(Control.self, from: d)
}

func writeControl(_ state: String, _ reason: String, interrupted: [Int] = []) {
    let c = Control(state: state, by: "Anton (AFP Runner)", at: ISO8601DateFormatter().string(from: Date()), reason: reason,
                    interrupted_runs: interrupted)
    let enc = JSONEncoder()
    enc.outputFormatting = [.prettyPrinted, .sortedKeys]
    if let d = try? enc.encode(c) { try? d.write(to: URL(fileURLWithPath: CONTROL), options: .atomic) }
    log("\(state): \(reason)")
}

// ---- The Mac's load ----

final class Load {
    private var last: [UInt64]? = nil

    /// Busy share of all cores since the last call, 0...1
    func cpu() -> Double {
        var count: natural_t = 0
        var info: processor_info_array_t? = nil
        var infoCount: mach_msg_type_number_t = 0
        guard host_processor_info(mach_host_self(), PROCESSOR_CPU_LOAD_INFO, &count, &info, &infoCount) == KERN_SUCCESS,
              let info else { return 0 }
        defer { vm_deallocate(mach_task_self_, vm_address_t(bitPattern: info), vm_size_t(Int(infoCount) * MemoryLayout<integer_t>.stride)) }
        var ticks: [UInt64] = [0, 0]  // busy, total
        for c in 0..<Int(count) {
            let base = c * Int(CPU_STATE_MAX)
            let user = UInt64(info[base + Int(CPU_STATE_USER)]), sys = UInt64(info[base + Int(CPU_STATE_SYSTEM)])
            let nice = UInt64(info[base + Int(CPU_STATE_NICE)]), idle = UInt64(info[base + Int(CPU_STATE_IDLE)])
            ticks[0] += user + sys + nice
            ticks[1] += user + sys + nice + idle
        }
        defer { last = ticks }
        guard let last, ticks[1] > last[1] else { return 0 }
        return Double(ticks[0] - last[0]) / Double(ticks[1] - last[1])
    }

    /// Memory in use (active, wired, compressed) and the Mac's total, in bytes
    func memory() -> (used: UInt64, total: UInt64) {
        var stats = vm_statistics64()
        var size = mach_msg_type_number_t(MemoryLayout<vm_statistics64>.stride / MemoryLayout<integer_t>.stride)
        let ok = withUnsafeMutablePointer(to: &stats) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(size)) { host_statistics64(mach_host_self(), HOST_VM_INFO64, $0, &size) }
        }
        let total = ProcessInfo.processInfo.physicalMemory
        guard ok == KERN_SUCCESS else { return (0, total) }
        let page = UInt64(vm_kernel_page_size)
        let used = (UInt64(stats.active_count) + UInt64(stats.wire_count) + UInt64(stats.compressor_page_count)) * page
        return (used, total)
    }
}

// ---- GitHub: the jobs running and waiting ----

struct Job: Identifiable, Hashable {
    let id: Int
    let run: Int
    let name: String
    let branch: String
    let status: String  // queued, in_progress, waiting
    let runner: String
    let started: Date?
}

func githubJobs() -> (jobs: [Job], online: String?)? {
    var jobs: [Job] = []
    var ok = false
    for status in ["in_progress", "queued"] {
        let r = run([GH, "api", "repos/\(REPO)/actions/runs?status=\(status)&per_page=20", "--jq",
                     ".workflow_runs[] | [.id, .head_branch] | @tsv"])
        guard r.status == 0 else { continue }
        ok = true
        for line in r.out.split(separator: "\n") {
            let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
            guard f.count >= 2, let runId = Int(f[0]) else { continue }
            let j = run([GH, "api", "repos/\(REPO)/actions/runs/\(runId)/jobs?per_page=50", "--jq",
                         ".jobs[] | select(.status != \"completed\") | [.id, .name, .status, (.runner_name // \"\"), (.started_at // \"\")] | @tsv"])
            for jl in j.out.split(separator: "\n") {
                let g = jl.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
                guard g.count >= 5, let jid = Int(g[0]) else { continue }
                jobs.append(Job(id: jid, run: runId, name: g[1], branch: f[1], status: g[2], runner: g[3],
                                started: ISO8601DateFormatter().date(from: g[4])))
            }
        }
    }
    let o = run([GH, "api", "repos/\(REPO)/actions/runners", "--jq", ".runners[] | select(.name == \"\(RUNNER_NAME)\") | .status"])
    return ok ? (jobs, o.status == 0 ? o.out.trimmingCharacters(in: .whitespacesAndNewlines) : nil) : nil
}

// ---- The model ----

@MainActor
final class Model: ObservableObject {
    @Published var cpu = 0.0
    @Published var mem: (used: UInt64, total: UInt64) = (0, 1)
    @Published var loaded = false
    @Published var working = false
    @Published var control: Control? = readControl()
    @Published var jobs: [Job] = []
    @Published var online: String? = nil
    @Published var githubChecked: Date? = nil
    @Published var githubFailed = false
    @Published var busyAction = false
    private let load = Load()
    private var timers: [Timer] = []

    init() {
        _ = load.cpu()
        refreshLocal()
        refreshGitHub()
        timers.append(Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in Task { @MainActor in self.refreshLocal() } })
        timers.append(Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { _ in Task { @MainActor in self.refreshGitHub() } })
    }

    var state: String {
        if control?.state == "stopping" && loaded { return "stopping" }
        if !loaded { return "stopped" }
        return working ? "working" : "idle"
    }

    var icon: String {
        switch state {
        case "stopped": return "pause.circle"
        case "stopping": return "hourglass"
        case "working": return "hammer.fill"
        default: return "hammer"
        }
    }

    func refreshLocal() {
        cpu = load.cpu()
        mem = load.memory()
        DispatchQueue.global().async {
            let loaded = run(["/bin/launchctl", "print", "\(DOMAIN)/\(LABEL)"]).status == 0
            let working = run(["/usr/bin/pgrep", "-x", "Runner.Worker"]).status == 0
            DispatchQueue.main.async {
                self.loaded = loaded
                self.working = working
                self.control = readControl()
            }
        }
    }

    func refreshGitHub() {
        DispatchQueue.global().async {
            let g = githubJobs()
            DispatchQueue.main.async {
                if let g { self.jobs = g.jobs; self.online = g.online; self.githubChecked = Date(); self.githubFailed = false }
                else { self.githubFailed = true }
            }
        }
    }

    // ---- Stop and Start ----

    func stopPressed() {
        guard loaded else { return }
        if working {
            NSApp.activate(ignoringOtherApps: true)
            let a = NSAlert()
            a.messageText = "The runner is running a job"
            a.informativeText = "Stop now: the job fails, and Start runs it again. After this job: the runner stops once it's done."
            a.addButton(withTitle: "After this job")
            a.addButton(withTitle: "Stop now")
            a.addButton(withTitle: "Cancel")
            switch a.runModal() {
            case .alertFirstButtonReturn: stopAfterJob()
            case .alertSecondButtonReturn: stopNow()
            default: return
            }
        } else {
            stopNow()
        }
    }

    func stopNow() {
        busyAction = true
        let mine = jobs.filter { $0.runner == RUNNER_NAME && $0.status == "in_progress" }.map(\.run)
        DispatchQueue.global().async {
            // Fresh: the job list may be up to 30 s old
            let now = (githubJobs()?.jobs ?? []).filter { $0.runner == RUNNER_NAME && $0.status == "in_progress" }.map(\.run)
            let cut = Array(Set(mine + now)).sorted()
            run(["/bin/launchctl", "disable", "\(DOMAIN)/\(LABEL)"])
            run(["/bin/launchctl", "bootout", "\(DOMAIN)/\(LABEL)"], timeout: 60)
            writeControl("stopped", cut.isEmpty ? "Stop pressed" : "Stop now pressed during a job", interrupted: cut)
            DispatchQueue.main.async { self.busyAction = false; self.refreshLocal(); self.refreshGitHub() }
        }
    }

    func stopAfterJob() {
        writeControl("stopping", "Stop after this job pressed")
        // Detached from this app (quitting it doesn't cancel the stop): waits for the job, then stops the runner
        let script = """
        while /usr/bin/pgrep -x Runner.Worker >/dev/null; do sleep 3; done
        /bin/launchctl disable \(DOMAIN)/\(LABEL); /bin/launchctl bootout \(DOMAIN)/\(LABEL)
        /usr/bin/sed -i '' -e 's/"state" : "stopping"/"state" : "stopped"/' -e 's/"reason" : "Stop after this job pressed"/"reason" : "Stopped after its job (Stop after this job)"/' '\(CONTROL)'
        echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) stopped after its job" >> '\(LOG)'
        """
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/sh")
        p.arguments = ["-c", "nohup /bin/sh -c \"$0\" >/dev/null 2>&1 &", script]
        try? p.run()
        p.waitUntilExit()
        refreshLocal()
    }

    func startPressed() {
        busyAction = true
        let rerun = control?.interrupted_runs ?? []
        DispatchQueue.global().async {
            // A "stop after this job" still waiting is called off
            run(["/usr/bin/pkill", "-f", "pgrep -x Runner.Worker >/dev/null; do sleep 3"])
            run(["/bin/launchctl", "enable", "\(DOMAIN)/\(LABEL)"])
            if run(["/bin/launchctl", "print", "\(DOMAIN)/\(LABEL)"]).status != 0 {
                run(["/bin/launchctl", "bootstrap", DOMAIN, RUNNER_PLIST], timeout: 60)
            }
            writeControl("running", "Start pressed")
            // The jobs Stop now cut short, run again (only those that ended failed or cancelled)
            for id in rerun {
                let c = run([GH, "run", "view", String(id), "-R", REPO, "--json", "status,conclusion", "--jq", ".status + \" \" + (.conclusion // \"\")"]).out
                if c.hasPrefix("completed") && (c.contains("failure") || c.contains("cancelled")) {
                    let r = run([GH, "run", "rerun", String(id), "-R", REPO, "--failed"])
                    log("rerun \(id): \(r.status == 0 ? "asked" : "failed")")
                }
            }
            DispatchQueue.main.async { self.busyAction = false; self.refreshLocal(); self.refreshGitHub() }
        }
    }
}

// ---- The panel ----

func gb(_ b: UInt64) -> String { String(format: "%.1f GB", Double(b) / 1_073_741_824) }

func ago(_ d: Date?) -> String {
    guard let d else { return "" }
    let m = Int(Date().timeIntervalSince(d) / 60)
    return m < 1 ? "just now" : m < 60 ? "\(m) min" : "\(m / 60) h \(m % 60) min"
}

struct Bar: View {
    let label: String
    let value: Double
    let text: String
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack { Text(label); Spacer(); Text(text).monospacedDigit().foregroundStyle(.secondary) }
            ProgressView(value: min(max(value, 0), 1))
        }
    }
}

struct Panel: View {
    @ObservedObject var m: Model
    @State private var atLogin = SMAppService.mainApp.status == .enabled

    var stateLine: String {
        switch m.state {
        case "stopped":
            if let c = m.control, c.state == "stopped" {
                return "Stopped by you, \(ago(ISO8601DateFormatter().date(from: c.at))) ago"
            }
            return "Stopped (not by this app)"
        case "stopping": return "Stopping after its job"
        case "working": return "Running a job"
        default: return "Idle, waiting for jobs"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: m.icon)
                Text("GitHub runner").font(.headline)
                Spacer()
                Text(m.online.map { "GitHub: \($0)" } ?? "").font(.caption).foregroundStyle(.secondary)
            }
            Text(stateLine)
            Bar(label: "CPU", value: m.cpu, text: "\(Int(m.cpu * 100)) %")
            Bar(label: "Memory", value: Double(m.mem.used) / Double(m.mem.total), text: "\(gb(m.mem.used)) of \(gb(m.mem.total))")
            Divider()
            let mine = m.jobs.filter { $0.runner == RUNNER_NAME && $0.status == "in_progress" }
            let waiting = m.jobs.filter { $0.status != "in_progress" }
            let elsewhere = m.jobs.filter { $0.status == "in_progress" && $0.runner != RUNNER_NAME }
            Text("On this Mac").font(.subheadline).bold()
            if mine.isEmpty { Text("Nothing").foregroundStyle(.secondary) }
            ForEach(mine) { j in Text("\(j.name) · \(j.branch) · \(ago(j.started))").lineLimit(1) }
            Text("Waiting (\(waiting.count))").font(.subheadline).bold()
            if waiting.isEmpty { Text("Nothing").foregroundStyle(.secondary) }
            ForEach(waiting.prefix(8)) { j in Text("\(j.name) · \(j.branch)").lineLimit(1) }
            if !elsewhere.isEmpty {
                Text("On GitHub's runners (\(elsewhere.count))").font(.subheadline).bold()
                ForEach(elsewhere.prefix(5)) { j in Text("\(j.name) · \(j.branch)").lineLimit(1).foregroundStyle(.secondary) }
            }
            Text(m.githubFailed ? "GitHub unreachable (gh)" : m.githubChecked.map { "GitHub checked \(ago($0)) ago" } ?? "Asking GitHub…")
                .font(.caption).foregroundStyle(.secondary)
            Divider()
            HStack {
                if m.state == "stopped" || m.state == "stopping" {
                    Button("Start") { m.startPressed() }.keyboardShortcut(.defaultAction)
                } else {
                    Button("Stop") { m.stopPressed() }
                }
                if m.busyAction { ProgressView().controlSize(.small) }
                Spacer()
                Button("Actions page") { NSWorkspace.shared.open(URL(string: "https://github.com/\(REPO)/actions")!) }
            }
            .disabled(m.busyAction)
            HStack {
                Toggle("Open at login", isOn: $atLogin)
                    .onChange(of: atLogin) { _, on in
                        do { if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() } }
                        catch { atLogin = SMAppService.mainApp.status == .enabled }
                    }
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
            }
            .font(.caption)
        }
        .padding(14)
        .frame(width: 340)
    }
}

/// `AFPRunner --status`: what the panel shows, as JSON on stdout (for Claude Code sessions and scripts), no menu bar
func printStatus() {
    let loaded = run(["/bin/launchctl", "print", "\(DOMAIN)/\(LABEL)"]).status == 0
    let working = run(["/usr/bin/pgrep", "-x", "Runner.Worker"]).status == 0
    let c = readControl()
    let state = !loaded ? "stopped" : c?.state == "stopping" ? "stopping" : working ? "working" : "idle"
    var out: [String: Any] = ["state": state, "service_loaded": loaded, "job_running_here": working]
    if let c { out["control"] = ["state": c.state, "by": c.by, "at": c.at, "reason": c.reason, "interrupted_runs": c.interrupted_runs] }
    out["stopped_by_anton"] = !loaded && c?.state == "stopped"
    if let g = githubJobs() {
        out["github_runner_status"] = g.online ?? ""
        out["jobs"] = g.jobs.map { ["run": $0.run, "name": $0.name, "branch": $0.branch, "status": $0.status, "runner": $0.runner] }
    } else {
        out["github"] = "unreachable"
    }
    let d = try! JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted, .sortedKeys])
    print(String(data: d, encoding: .utf8)!)
}

@main
struct Main {
    static func main() {
        if CommandLine.arguments.contains("--status") { printStatus(); return }
        RunnerBar.main()
    }
}

struct RunnerBar: App {
    @StateObject private var m = Model()
    var body: some Scene {
        MenuBarExtra {
            Panel(m: m)
        } label: {
            Image(systemName: m.icon)
        }
        .menuBarExtraStyle(.window)
    }
}
