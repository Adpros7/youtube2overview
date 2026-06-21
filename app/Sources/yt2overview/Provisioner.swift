import Foundation

/// Resolves / installs the Python + rapid-mlx[vision] runtime. The heavy first-run
/// install flow is implemented in Phase 10; this exposes the resolved paths.
enum Provisioner {
    /// Where we keep the app's private venv.
    static var venvDir: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("yt2overview/venv", isDirectory: true)
    }

    /// Path to the provisioned `rapid-mlx`, if the venv exists.
    static func provisionedMlxBin() -> String? {
        let bin = venvDir.appendingPathComponent("bin/rapid-mlx").path
        return FileManager.default.isExecutableFile(atPath: bin) ? bin : nil
    }
}
