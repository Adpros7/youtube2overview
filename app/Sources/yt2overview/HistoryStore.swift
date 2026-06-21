import Foundation
import Observation

/// One saved generation.
struct HistoryEntry: Codable, Identifiable, Equatable {
    var id: String
    var url: String
    var title: String
    var channel: String
    var date: Date
    var result: JobResult

    init(url: String, result: JobResult) {
        self.id = UUID().uuidString
        self.url = url
        self.title = result.data.meta.title.isEmpty ? url : result.data.meta.title
        self.channel = result.data.meta.channel
        self.date = Date()
        self.result = result
    }
}

/// Persists recent generations to Application Support as JSON.
@MainActor
@Observable
final class HistoryStore {
    private(set) var entries: [HistoryEntry] = []
    private let maxEntries = 100

    private nonisolated static var fileURL: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("yt2overview/history.json")
    }

    init() {
        load()
    }

    func add(url: String, result: JobResult) {
        let entry = HistoryEntry(url: url, result: result)
        // De-dupe by URL: drop a prior entry for the same video.
        entries.removeAll { $0.url == entry.url }
        entries.insert(entry, at: 0)
        if entries.count > maxEntries { entries = Array(entries.prefix(maxEntries)) }
        save()
    }

    func delete(_ entry: HistoryEntry) {
        entries.removeAll { $0.id == entry.id }
        save()
    }

    func clear() {
        entries.removeAll()
        save()
    }

    // MARK: - Persistence

    private func load() {
        guard let data = try? Data(contentsOf: Self.fileURL) else { return }
        if let decoded = try? JSON.decoder.decode([HistoryEntry].self, from: data) {
            entries = decoded
        }
    }

    private func save() {
        let url = Self.fileURL
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        if let data = try? JSON.encoder.encode(entries) {
            try? data.write(to: url, options: .atomic)
        }
    }
}
