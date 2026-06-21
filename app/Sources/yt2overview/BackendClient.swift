import Foundation

enum BackendError: LocalizedError {
    case binaryNotFound
    case notReady
    case http(String)
    var errorDescription: String? {
        switch self {
        case .binaryNotFound: return "Backend binary not found in app bundle."
        case .notReady: return "Backend is not ready."
        case .http(let m): return m
        }
    }
}

/// Launches and talks to the Rust backend process.
final class BackendClient: @unchecked Sendable {
    private(set) var baseURL: URL?
    private var process: Process?
    private let session = URLSession(configuration: .default)

    /// Locate the backend binary: bundled first, then dev fallbacks.
    private func resolveBinary() -> URL? {
        if let url = Bundle.main.url(forResource: "yt2overview-backend", withExtension: nil, subdirectory: "bin") {
            return url
        }
        if let url = Bundle.main.url(forResource: "yt2overview-backend", withExtension: nil) {
            return url
        }
        if let env = ProcessInfo.processInfo.environment["YT2O_BACKEND_BIN"],
           FileManager.default.isExecutableFile(atPath: env) {
            return URL(fileURLWithPath: env)
        }
        // Dev fallback relative to the running executable: app/.build/.../debug/yt2overview
        let exe = Bundle.main.executableURL ?? URL(fileURLWithPath: CommandLine.arguments[0])
        let candidates = [
            exe.deletingLastPathComponent()
                .appendingPathComponent("../../../../backend/target/release/yt2overview-backend"),
            exe.deletingLastPathComponent()
                .appendingPathComponent("../../../../backend/target/debug/yt2overview-backend"),
        ]
        for c in candidates where FileManager.default.isExecutableFile(atPath: c.path) {
            return c.standardizedFileURL
        }
        return nil
    }

    /// Extra environment passed to the backend (bundled tools + provisioned mlx).
    private func childEnvironment() -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        if let binDir = Bundle.main.resourceURL?.appendingPathComponent("bin").path,
           FileManager.default.fileExists(atPath: binDir) {
            env["YT2O_BIN_DIR"] = binDir
        }
        // Stable venv dir: the backend re-checks this each job, so provisioning that
        // finishes *after* launch is still picked up.
        env["YT2O_VENV_DIR"] = Provisioner.venvDir.path
        return env
    }

    /// Start the backend and wait until it announces its listening URL.
    func start() async throws {
        if baseURL != nil { return }
        guard let bin = resolveBinary() else { throw BackendError.binaryNotFound }

        let proc = Process()
        proc.executableURL = bin
        proc.environment = childEnvironment()
        let stdout = Pipe()
        proc.standardOutput = stdout
        proc.standardError = Pipe()

        let urlBox = URLBox()
        stdout.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let s = String(data: data, encoding: .utf8) else { return }
            for line in s.split(separator: "\n") {
                if line.hasPrefix("YT2O_LISTENING ") {
                    let urlStr = line.replacingOccurrences(of: "YT2O_LISTENING ", with: "")
                    if let url = URL(string: urlStr.trimmingCharacters(in: .whitespaces)) {
                        urlBox.set(url)
                    }
                }
            }
        }

        try proc.run()
        self.process = proc

        // Wait up to ~10s for the listening handshake.
        for _ in 0..<100 {
            if let url = urlBox.get() {
                self.baseURL = url
                return
            }
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
        throw BackendError.notReady
    }

    func stop() {
        process?.terminate()
        process = nil
        baseURL = nil
    }

    // MARK: - API

    func health() async -> Bool {
        guard let base = baseURL else { return false }
        let (_, resp) = (try? await session.data(from: base.appendingPathComponent("health"))) ?? (Data(), URLResponse())
        return (resp as? HTTPURLResponse)?.statusCode == 200
    }

    func models() async throws -> [CachedModel] {
        guard let base = baseURL else { throw BackendError.notReady }
        let (data, _) = try await session.data(from: base.appendingPathComponent("models"))
        struct Resp: Codable { var cached: [CachedModel] }
        return try JSON.decoder.decode(Resp.self, from: data).cached
    }

    func process(url: String, settings: Settings) async throws -> String {
        guard let base = baseURL else { throw BackendError.notReady }
        struct Req: Codable { var url: String; var settings: Settings }
        var req = URLRequest(url: base.appendingPathComponent("process"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSON.encoder.encode(Req(url: url, settings: settings))
        let (data, resp) = try await session.data(for: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            throw BackendError.http(String(data: data, encoding: .utf8) ?? "process failed")
        }
        struct Resp: Codable { var jobId: String }
        return try JSON.decoder.decode(Resp.self, from: data).jobId
    }

    /// Stream SSE progress events for a job as an async sequence.
    func events(jobId: String) -> AsyncThrowingStream<ProgressEvent, Error> {
        let base = baseURL
        let session = session
        return AsyncThrowingStream { continuation in
            let task = Task {
                guard let base else {
                    continuation.finish(throwing: BackendError.notReady)
                    return
                }
                let url = base.appendingPathComponent("events").appendingPathComponent(jobId)
                do {
                    let (bytes, _) = try await session.bytes(from: url)
                    for try await line in bytes.lines {
                        guard line.hasPrefix("data:") else { continue }
                        let json = line.dropFirst(5).trimmingCharacters(in: .whitespaces)
                        if let data = json.data(using: .utf8),
                           let ev = try? JSON.decoder.decode(ProgressEvent.self, from: data) {
                            continuation.yield(ev)
                            if ev.kind == "done" || ev.kind == "error" { break }
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func result(jobId: String) async throws -> JobResult? {
        guard let base = baseURL else { throw BackendError.notReady }
        let url = base.appendingPathComponent("result").appendingPathComponent(jobId)
        let (data, _) = try await session.data(from: url)
        struct Resp: Codable {
            var status: String
            var result: JobResult?
            var error: String?
        }
        let r = try JSON.decoder.decode(Resp.self, from: data)
        if r.status == "error" { throw BackendError.http(r.error ?? "job failed") }
        return r.result
    }
}

/// Thread-safe box for the listening URL captured off the pipe handler.
private final class URLBox: @unchecked Sendable {
    private let lock = NSLock()
    private var url: URL?
    func set(_ u: URL) { lock.lock(); url = u; lock.unlock() }
    func get() -> URL? { lock.lock(); defer { lock.unlock() }; return url }
}
