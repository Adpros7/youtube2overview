import AppKit
import Foundation
import Observation
import SwiftUI

enum Phase: Equatable {
    case idle
    case starting
    case running(stage: String, message: String, progress: Double)
    case done
    case failed(String)
}

@MainActor
@Observable
final class AppModel {
    var url: String = ""
    var settings = Settings()
    var phase: Phase = .idle
    var result: JobResult?
    var cachedModels: [CachedModel] = []
    var showSettings = false
    var pendingLocalFiles: [URL] = []
    var batchTotal = 0
    var batchCompleted = 0

    /// "human" or "ai"
    var outputMode: String = "human"

    let provisioner = Provisioner()
    let history = HistoryStore()
    var showHistory = false
    private let backend = BackendClient()
    private var started = false

    var hasResult: Bool { result != nil }

    var isBusy: Bool {
        switch phase {
        case .starting, .running: return true
        default: return false
        }
    }

    var progressValue: Double {
        if case let .running(_, _, p) = phase { return p }
        if case .done = phase { return 1 }
        return 0
    }

    func bootIfNeeded() async {
        guard !started else { return }
        started = true
        // Provision the model runtime in the background; the UI shows a banner while it runs.
        Task { await provisioner.ensureReady() }
        do {
            try await backend.start()
            cachedModels = (try? await backend.models()) ?? []
            // Prefer an already-cached multimodal model as the default.
            if let preferred = cachedModels.first(where: { $0.multimodal && $0.repo.contains("gemma-4") })
                ?? cachedModels.first(where: { $0.multimodal }) {
                settings.model = preferred.repo
            }
        } catch {
            phase = .failed("Could not start backend: \(error.localizedDescription)")
        }
    }

    func generate() {
        let target = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !target.isEmpty, !isBusy else { return }
        pendingLocalFiles.removeAll()
        batchTotal = 0
        batchCompleted = 0
        startJob(target, advancesBatch: false)
    }

    private func startJob(_ target: String, advancesBatch: Bool) {
        result = nil
        phase = .starting
        Task { await runJob(target, advancesBatch: advancesBatch) }
    }

    /// Use a locally-picked/dropped audio or video file as the source and start a job.
    func useLocalFile(_ fileURL: URL) {
        useLocalFiles([fileURL])
    }

    /// Add one or more locally-picked/dropped media files to the processing queue.
    func useLocalFiles(_ fileURLs: [URL]) {
        let files = fileURLs.filter { $0.isFileURL }
        guard !files.isEmpty else { return }

        if !isBusy && pendingLocalFiles.isEmpty {
            batchCompleted = 0
            batchTotal = files.count
        } else {
            batchTotal += files.count
        }

        pendingLocalFiles.append(contentsOf: files)
        startNextQueuedLocalFile()
    }

    private func startNextQueuedLocalFile() {
        guard !isBusy, !pendingLocalFiles.isEmpty else { return }
        let fileURL = pendingLocalFiles.removeFirst()
        url = fileURL.path
        startJob(fileURL.path, advancesBatch: true)
    }

    /// Filename to show when the current input is a local file (vs. a web URL).
    var localFileLabel: String? {
        let t = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty, !t.hasPrefix("http"),
              FileManager.default.fileExists(atPath: t) else { return nil }
        return (t as NSString).lastPathComponent
    }

    var batchLabel: String? {
        guard batchTotal > 1 else { return nil }
        if isBusy {
            let current = min(batchCompleted + 1, batchTotal)
            let remaining = max(batchTotal - current, 0)
            return remaining == 0
                ? "Processing \(current) of \(batchTotal)"
                : "Processing \(current) of \(batchTotal) · \(remaining) queued"
        }
        if batchCompleted >= batchTotal {
            return "Processed \(batchCompleted) files"
        }
        if !pendingLocalFiles.isEmpty {
            return "\(pendingLocalFiles.count) files queued"
        }
        return nil
    }

    private func runJob(_ target: String, advancesBatch: Bool) async {
        do {
            if backend.baseURL == nil { try await backend.start() }
            // Make sure the model runtime is installed before a job needs it.
            await provisioner.ensureReady()
            let jobId = try await backend.process(url: target, settings: settings)
            phase = .running(stage: "start", message: "Starting…", progress: 0.02)
            for try await ev in backend.events(jobId: jobId) {
                if ev.kind == "error" {
                    phase = .failed(ev.message)
                } else if ev.kind != "done" {
                    phase = .running(stage: ev.stage, message: ev.message, progress: ev.progress)
                }
            }
            if case .failed = phase { return }
            if let res = try await backend.result(jobId: jobId) {
                result = res
                phase = .done
                history.add(url: target, result: res)
            } else {
                phase = .failed("No result produced.")
            }
        } catch {
            phase = .failed(error.localizedDescription)
        }

        if case .done = phase {
            if advancesBatch {
                batchCompleted += 1
            }
            startNextQueuedLocalFile()
        }
    }

    func currentOutput() -> String {
        guard let r = result else { return "" }
        return outputMode == "ai" ? r.outputs.aiPayload : r.outputs.humanMarkdown
    }

    // MARK: - Menu actions

    func copyCurrent() { Clipboard.copy(currentOutput()) }
    func copyAI() { if let r = result { Clipboard.copy(r.outputs.aiPayload) } }

    func clear() {
        url = ""
        result = nil
        phase = .idle
        pendingLocalFiles.removeAll()
        batchTotal = 0
        batchCompleted = 0
    }

    func pasteURL() {
        if let s = NSPasteboard.general.string(forType: .string) {
            url = s.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    /// Load a previously-generated result back into the main view.
    func load(_ entry: HistoryEntry) {
        url = entry.url
        result = entry.result
        outputMode = "human"
        phase = .done
        showHistory = false
    }
}
