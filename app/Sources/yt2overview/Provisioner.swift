import Foundation
import Observation

enum ProvisionState: Equatable {
    case unknown
    case checking
    case installing(String)
    case ready
    case failed(String)
}

/// Resolves / installs the Python + rapid-mlx[vision] runtime into an app-private venv.
@MainActor
@Observable
final class Provisioner {
    var state: ProvisionState = .unknown

    /// Where we keep the app's private venv.
    nonisolated static var venvDir: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("yt2overview/venv", isDirectory: true)
    }

    /// Path to the provisioned `rapid-mlx`, if the venv exists.
    nonisolated static func provisionedMlxBin() -> String? {
        let bin = venvDir.appendingPathComponent("bin/rapid-mlx").path
        return FileManager.default.isExecutableFile(atPath: bin) ? bin : nil
    }

    /// Path to the provisioned `mlx_whisper`, if the venv exists.
    nonisolated static func provisionedWhisperBin() -> String? {
        let bin = venvDir.appendingPathComponent("bin/mlx_whisper").path
        return FileManager.default.isExecutableFile(atPath: bin) ? bin : nil
    }

    nonisolated static func provisionedRuntimeReady() -> Bool {
        provisionedMlxBin() != nil && provisionedWhisperBin() != nil
    }

    var isReady: Bool { state == .ready }

    /// Ensure a vision-capable rapid-mlx is available. Installs it on first run.
    func ensureReady() async {
        if case .ready = state { return }
        state = .checking

        // Dev shortcut: an explicit rapid-mlx provided via env (e.g. a prebuilt venv).
        if let mlx = ProcessInfo.processInfo.environment["YT2O_MLX_BIN"],
           FileManager.default.isExecutableFile(atPath: mlx) {
            let whisper = URL(fileURLWithPath: mlx)
                .deletingLastPathComponent()
                .appendingPathComponent("mlx_whisper")
            if FileManager.default.isExecutableFile(atPath: whisper.path) {
                state = .ready
                return
            }
        }
        if Self.provisionedRuntimeReady() {
            state = .ready
            return
        }

        guard let uv = findExecutable("uv") else {
            state = .failed("`uv` not found. Install uv to enable the local model runtime.")
            return
        }

        do {
            try FileManager.default.createDirectory(
                at: Self.venvDir.deletingLastPathComponent(), withIntermediateDirectories: true)

            if Self.provisionedMlxBin() == nil && Self.provisionedWhisperBin() == nil {
                state = .installing("Creating Python environment…")
                try await run(uv, ["venv", Self.venvDir.path, "--python", "3.12", "--seed"])
            }

            state = .installing("Installing rapid-mlx (vision) + whisper… this can take a few minutes")
            try await run(
                uv,
                ["pip", "install", "--python", Self.venvDir.appendingPathComponent("bin/python").path,
                 "rapid-mlx[vision]", "mlx-whisper"],
                onLine: { [weak self] line in
                    guard let self else { return }
                    if let pretty = Self.prettyInstallLine(line) {
                        self.state = .installing(pretty)
                    }
                })

            if Self.provisionedRuntimeReady() {
                state = .ready
            } else {
                state = .failed("Install finished but rapid-mlx or mlx_whisper was not found.")
            }
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    // MARK: - Process helpers

    private func run(_ exe: String, _ args: [String],
                     onLine: (@MainActor @Sendable (String) -> Void)? = nil) async throws {
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: exe)
            proc.arguments = args
            var env = ProcessInfo.processInfo.environment
            env["VIRTUAL_ENV"] = nil
            proc.environment = env

            let pipe = Pipe()
            proc.standardOutput = pipe
            proc.standardError = pipe
            if let onLine {
                pipe.fileHandleForReading.readabilityHandler = { handle in
                    guard let s = String(data: handle.availableData, encoding: .utf8) else { return }
                    for line in s.split(whereSeparator: \.isNewline) where !line.isEmpty {
                        let captured = String(line)
                        Task { @MainActor in onLine(captured) }
                    }
                }
            }
            proc.terminationHandler = { p in
                pipe.fileHandleForReading.readabilityHandler = nil
                if p.terminationStatus == 0 {
                    cont.resume()
                } else {
                    let out = (try? pipe.fileHandleForReading.readToEnd()).flatMap { String(data: $0, encoding: .utf8) } ?? ""
                    cont.resume(throwing: NSError(domain: "Provisioner", code: Int(p.terminationStatus),
                                                  userInfo: [NSLocalizedDescriptionKey: "uv failed (\(p.terminationStatus)). \(out.suffix(300))"]))
                }
            }
            do { try proc.run() } catch { cont.resume(throwing: error) }
        }
    }

    private static func prettyInstallLine(_ line: String) -> String? {
        if line.contains("Resolved") || line.contains("Prepared") || line.contains("Installed")
            || line.contains("Downloading") || line.contains("Building") {
            return "Installing runtime… \(line.trimmingCharacters(in: .whitespaces))"
        }
        return nil
    }

    private func findExecutable(_ name: String) -> String? {
        // Bundled first.
        if let url = Bundle.main.resourceURL?.appendingPathComponent("bin/\(name)"),
           FileManager.default.isExecutableFile(atPath: url.path) {
            return url.path
        }
        // Common locations + PATH.
        let candidates = ["/opt/homebrew/bin/\(name)", "/usr/local/bin/\(name)", "/usr/bin/\(name)"]
        for c in candidates where FileManager.default.isExecutableFile(atPath: c) { return c }
        // `which` via login shell.
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        proc.arguments = [name]
        let pipe = Pipe(); proc.standardOutput = pipe
        try? proc.run(); proc.waitUntilExit()
        if let out = try? pipe.fileHandleForReading.readToEnd(),
           let s = String(data: out, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
           !s.isEmpty, FileManager.default.isExecutableFile(atPath: s) {
            return s
        }
        return nil
    }
}
