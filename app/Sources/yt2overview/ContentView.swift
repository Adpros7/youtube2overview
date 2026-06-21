import SwiftUI

struct ContentView: View {
    @State private var model = AppModel()

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.15)
            ScrollView {
                VStack(spacing: 20) {
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
                Text("Local YouTube → AI-ready overview")
                    .font(.system(size: 11)).foregroundStyle(.secondary)
            }
            Spacer()
            Button {
                model.showSettings = true
            } label: {
                Image(systemName: "slider.horizontal.3").font(.system(size: 15, weight: .semibold))
            }
            .buttonStyle(.glass)
            .help("Settings")
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .padding(.top, 18)
    }

    // MARK: Input

    private var inputCard: some View {
        GlassCard {
            VStack(alignment: .leading, spacing: 14) {
                Text("Paste a YouTube link")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    Image(systemName: "link").foregroundStyle(.secondary)
                    TextField("https://youtube.com/watch?v=…", text: $model.url)
                        .textFieldStyle(.plain)
                        .font(.system(size: 15))
                        .onSubmit { model.generate() }
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
            }
        }
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
