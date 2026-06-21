import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @Bindable var model: AppModel
    @State private var showImporter = false
    @State private var dropTargeted = false

    private var mediaContentTypes: [UTType] {
        var types: [UTType] = [.movie, .video, .audio]
        for ext in ["mp4", "m4v", "mov", "m4a", "mp3", "wav", "aiff", "aac", "flac", "webm", "mkv", "avi"] {
            if let type = UTType(filenameExtension: ext) {
                types.append(type)
            }
        }
        return types
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.15)
            ScrollView {
                VStack(spacing: 20) {
                    provisioningBanner
                    inputCard
                    content
                }
                .padding(28)
                .frame(maxWidth: 900)
                .frame(maxWidth: .infinity)
            }
        }
        .task { await model.bootIfNeeded() }
        .sheet(isPresented: $model.showSettings) {
            SettingsView(model: model)
        }
        .sheet(isPresented: $model.showHistory) {
            HistoryView(model: model)
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(LinearGradient(colors: [Theme.accent, Theme.violet],
                                         startPoint: .topLeading, endPoint: .bottomTrailing))
                    .frame(width: 34, height: 34)
                Image(systemName: "play.rectangle.fill")
                    .foregroundStyle(.white)
                    .font(.system(size: 17, weight: .bold))
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("yt2overview").font(.system(size: 16, weight: .bold))
                Text("Local media → AI-ready overview")
                    .font(.system(size: 11)).foregroundStyle(.secondary)
            }
            Spacer()
            Button {
                model.showHistory = true
            } label: {
                Image(systemName: "clock.arrow.circlepath").font(.system(size: 15, weight: .semibold))
            }
            .buttonStyle(.glass)
            .help("History (⌘Y)")
            Button {
                model.showSettings = true
            } label: {
                Image(systemName: "slider.horizontal.3").font(.system(size: 15, weight: .semibold))
            }
            .buttonStyle(.glass)
            .help("Settings (⌘,)")
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .padding(.top, 18)
    }

    // MARK: Provisioning banner

    @ViewBuilder private var provisioningBanner: some View {
        switch model.provisioner.state {
        case .checking, .installing:
            GlassCard(corner: 16, padding: 14) {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Setting up the local AI runtime").font(.system(size: 12, weight: .semibold))
                        Text(provisioningMessage).font(.system(size: 10.5)).foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Spacer()
                }
            }
        case .failed(let msg):
            GlassCard(corner: 16, padding: 14) {
                HStack(spacing: 10) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    Text("Runtime setup failed: \(msg)").font(.system(size: 11)).foregroundStyle(.secondary)
                    Spacer()
                }
            }
        default:
            EmptyView()
        }
    }

    private var provisioningMessage: String {
        switch model.provisioner.state {
        case .installing(let m): return m
        case .checking: return "Checking…"
        default: return ""
        }
    }

    // MARK: Input

    private var inputCard: some View {
        GlassCard {
            VStack(alignment: .leading, spacing: 14) {
                Text("Paste a media link — or add audio/video files")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    Image(systemName: "link").foregroundStyle(.secondary)
                    TextField("https://youtube.com/watch?v=…", text: $model.url)
                        .textFieldStyle(.plain)
                        .font(.system(size: 15))
                        .onSubmit { model.generate() }
                    Button {
                        showImporter = true
                    } label: {
                        Image(systemName: "square.and.arrow.up")
                            .font(.system(size: 14, weight: .semibold))
                    }
                    .buttonStyle(.glass)
                    .help("Upload local audio or video files")
                    .disabled(model.isBusy)
                    Button {
                        model.generate()
                    } label: {
                        Label(model.isBusy ? "Working…" : "Generate", systemImage: "sparkles")
                            .font(.system(size: 14, weight: .semibold))
                    }
                    .buttonStyle(.glassProminent)
                    .tint(Theme.accent)
                    .disabled(model.isBusy || model.url.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .padding(12)
                .glassPanel()
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(Theme.accent, lineWidth: dropTargeted ? 2 : 0)
                )
                if let name = model.localFileLabel {
                    HStack(spacing: 6) {
                        Image(systemName: "waveform").foregroundStyle(Theme.violet)
                        Text(name).lineLimit(1).truncationMode(.middle)
                        Spacer()
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                }
                if let batch = model.batchLabel {
                    HStack(spacing: 6) {
                        Image(systemName: "square.stack.3d.up").foregroundStyle(Theme.accent)
                        Text(batch).lineLimit(1)
                        Spacer()
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                }
            }
        }
        .onDrop(of: [.fileURL], isTargeted: $dropTargeted) { providers in
            handleDrop(providers)
        }
        .fileImporter(
            isPresented: $showImporter,
            allowedContentTypes: mediaContentTypes,
            allowsMultipleSelection: true
        ) { result in
            if case .success(let urls) = result {
                model.useLocalFiles(urls)
            }
        }
    }

    /// Accept dropped audio/video file URLs and add them to the processing queue.
    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        let fileProviders = providers.filter { $0.canLoadObject(ofClass: URL.self) }
        guard !fileProviders.isEmpty else { return false }

        let accumulator = URLDropAccumulator()
        let group = DispatchGroup()

        for provider in fileProviders {
            group.enter()
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                if let url {
                    accumulator.append(url)
                }
                group.leave()
            }
        }

        group.notify(queue: .main) {
            model.useLocalFiles(accumulator.snapshot())
        }
        return true
    }

    // MARK: Body content (placeholder / progress / results)

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .idle:
            placeholder
        case .starting, .running:
            ProgressCard(model: model)
        case .failed(let msg):
            ErrorCard(message: msg)
        case .done:
            if let result = model.result {
                ResultsView(model: model, result: result)
            }
        }
    }

    private var placeholder: some View {
        GlassCard(padding: 40) {
            VStack(spacing: 10) {
                Image(systemName: "wand.and.stars")
                    .font(.system(size: 34))
                    .foregroundStyle(Theme.violet)
                Text("Your overview will appear here")
                    .font(.system(size: 15, weight: .medium))
                Text("Transcript · top comments · visual overview · AI summary — all generated locally.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
        }
    }
}

private final class URLDropAccumulator: @unchecked Sendable {
    private let lock = NSLock()
    private var urls: [URL] = []

    func append(_ url: URL) {
        lock.lock()
        urls.append(url)
        lock.unlock()
    }

    func snapshot() -> [URL] {
        lock.lock()
        defer { lock.unlock() }
        return urls
    }
}

// MARK: - Progress

struct ProgressCard: View {
    var model: AppModel

    var body: some View {
        GlassCard {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    ProgressView().controlSize(.small)
                    Text(message).font(.system(size: 14, weight: .medium))
                    Spacer()
                    Text("\(Int(model.progressValue * 100))%")
                        .font(.system(size: 13, weight: .semibold).monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                ProgressView(value: model.progressValue)
                    .tint(Theme.accent)
            }
        }
    }

    private var message: String {
        if case let .running(_, msg, _) = model.phase { return msg }
        return "Starting…"
    }
}

struct ErrorCard: View {
    var message: String
    var body: some View {
        GlassCard {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange).font(.system(size: 18))
                VStack(alignment: .leading, spacing: 4) {
                    Text("Something went wrong").font(.system(size: 14, weight: .semibold))
                    Text(message).font(.system(size: 12)).foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
            }
        }
    }
}
