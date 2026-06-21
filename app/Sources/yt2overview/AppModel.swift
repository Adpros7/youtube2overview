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

    /// "human" or "ai"
    var outputMode: String = "human"

    private let backend = BackendClient()
    private var started = false

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
        result = nil
        phase = .starting
        Task { await runJob(target) }
    }

    private func runJob(_ target: String) async {
        do {
            if backend.baseURL == nil { try await backend.start() }
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
            } else {
                phase = .failed("No result produced.")
            }
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func currentOutput() -> String {
        guard let r = result else { return "" }
        return outputMode == "ai" ? r.outputs.aiPayload : r.outputs.humanMarkdown
    }
}
