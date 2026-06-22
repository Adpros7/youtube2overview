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
    private var pendingLocalFileJobIDs: [UUID] = []
    var localFileJobs: [LocalFileJob] = []
    var batchTotal = 0
    var batchCompleted = 0

    /// "human" or "ai"
    var outputMode: String = "human"

    let provisioner = Provisioner()
    let history = HistoryStore()
    var showHistory = false
    private let backend = BackendClient()
    private var started = false
    private var activeLocalFileJobIDs = Set<UUID>()
    private var localFileTasks: [UUID: Task<Void, Never>] = [:]
    private let maxConcurrentLocalFileJobs = 2

    var hasResult: Bool { result != nil }

    var isBusy: Bool {
        if !activeLocalFileJobIDs.isEmpty { return true }
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
        pendingLocalFileJobIDs.removeAll()
        localFileJobs.removeAll()
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

    /// Add one or more locally-picked/dropped media files to a bounded parallel batch.
    func useLocalFiles(_ fileURLs: [URL]) {
        let files = fileURLs.filter { $0.isFileURL }
        guard !files.isEmpty else { return }

        if pendingLocalFileJobIDs.isEmpty && activeLocalFileJobIDs.isEmpty {
            batchCompleted = 0
            batchTotal = files.count
            localFileJobs.removeAll()
        } else {
            batchTotal += files.count
        }

        let jobs = files.map { LocalFileJob(fileURL: $0) }
        localFileJobs.append(contentsOf: jobs)
        pendingLocalFileJobIDs.append(contentsOf: jobs.map(\.id))
        startQueuedLocalFiles()
    }

    private func startQueuedLocalFiles() {
        while activeLocalFileJobIDs.count < maxConcurrentLocalFileJobs,
              !pendingLocalFileJobIDs.isEmpty {
            let id = pendingLocalFileJobIDs.removeFirst()
            guard let index = localFileJobs.firstIndex(where: { $0.id == id }) else { continue }
            let fileURL = localFileJobs[index].fileURL
            localFileJobs[index].status = .running(
                stage: "start", message: "Starting…", progress: 0.02
            )
            activeLocalFileJobIDs.insert(id)
            let task = Task { await runLocalFileJob(id: id, target: fileURL.path) }
            localFileTasks[id] = task
        }
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
            let active = activeLocalFileJobIDs.count
            let queued = pendingLocalFileJobIDs.count
            return "\(active) processing in parallel · \(queued) queued"
        }
        if batchCompleted >= batchTotal {
            return "Processed \(batchCompleted) files"
        }
        if !pendingLocalFileJobIDs.isEmpty {
            return "\(pendingLocalFileJobIDs.count) files queued"
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
            startQueuedLocalFiles()
        }
    }

    private func runLocalFileJob(id: UUID, target: String) async {
        defer {
            activeLocalFileJobIDs.remove(id)
            localFileTasks.removeValue(forKey: id)
            startQueuedLocalFiles()
        }

        do {
            if backend.baseURL == nil { try await backend.start() }
            await provisioner.ensureReady()
            let jobId = try await backend.process(url: target, settings: settings)
            guard let index = localFileJobs.firstIndex(where: { $0.id == id }) else { return }
            localFileJobs[index].backendJobID = jobId
            if Task.isCancelled {
                try? await backend.cancel(jobId: jobId)
                localFileJobs[index].status = .cancelled
                return
            }
            for try await ev in backend.events(jobId: jobId) {
                guard let index = localFileJobs.firstIndex(where: { $0.id == id }) else { return }
                if ev.kind == "error" {
                    localFileJobs[index].status = .failed(ev.message)
                    return
                }
                if ev.kind != "done" {
                    localFileJobs[index].status = .running(
                        stage: ev.stage, message: ev.message, progress: ev.progress
                    )
                }
            }
            guard let index = localFileJobs.firstIndex(where: { $0.id == id }) else { return }
            if let res = try await backend.result(jobId: jobId) {
                localFileJobs[index].status = .done
                result = res
                phase = .done
                batchCompleted += 1
                history.add(url: target, result: res)
            } else {
                localFileJobs[index].status = .failed("No result produced.")
            }
        } catch {
            if let index = localFileJobs.firstIndex(where: { $0.id == id }) {
                localFileJobs[index].status = Task.isCancelled
                    ? .cancelled
                    : .failed(error.localizedDescription)
            }
        }
    }

    func cancelLocalFileJob(_ id: UUID) {
        guard let index = localFileJobs.firstIndex(where: { $0.id == id }) else { return }
        switch localFileJobs[index].status {
        case .queued:
            pendingLocalFileJobIDs.removeAll { $0 == id }
            localFileJobs[index].status = .cancelled
        case .running:
            localFileJobs[index].status = .running(
                stage: "cancelling", message: "Cancelling…", progress: 0
            )
            if let jobId = localFileJobs[index].backendJobID {
                Task { try? await backend.cancel(jobId: jobId) }
            }
            localFileTasks[id]?.cancel()
        case .done, .cancelled, .failed:
            break
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
        for job in localFileJobs {
            if let jobId = job.backendJobID {
                Task { try? await backend.cancel(jobId: jobId) }
            }
            localFileTasks[job.id]?.cancel()
        }
        url = ""
        result = nil
        phase = .idle
        pendingLocalFileJobIDs.removeAll()
        localFileJobs.removeAll()
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
