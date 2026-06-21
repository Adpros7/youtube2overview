import SwiftUI

/// Granular controls over every part of the pipeline, bound to `model.settings`.
struct SettingsView: View {
    @Bindable var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.15)
            ScrollView {
                VStack(spacing: 16) {
                    modelSection
                    overviewSection
                    commentsSection
                    visualSection
                    transcriptSection
                    sectionsSection
                    resetRow
                }
                .padding(18)
            }
        }
        .frame(width: 500, height: 640)
        .background(MicaBackground().ignoresSafeArea())
    }

    private var header: some View {
        HStack {
            Image(systemName: "slider.horizontal.3")
            Text("Settings").font(.system(size: 15, weight: .bold))
            Spacer()
            Button("Done") { dismiss() }
                .buttonStyle(.glassProminent).tint(Theme.accent)
        }
        .padding(16)
    }

    // MARK: Model

    private var modelSection: some View {
        SettingsGroup("Model", icon: "cpu") {
            HStack {
                Text("Model").settingLabel()
                Spacer()
                Picker("", selection: $model.settings.model) {
                    ForEach(model.cachedModels) { m in
                        Text(label(for: m)).tag(m.repo)
                    }
                    if !model.cachedModels.contains(where: { $0.repo == model.settings.model }) {
                        Text(model.settings.model).tag(model.settings.model)
                    }
                }
                .labelsHidden()
                .frame(maxWidth: 260)
            }
            if let m = model.cachedModels.first(where: { $0.repo == model.settings.model }), !m.multimodal {
                Label("Text-only model — visual overview will be skipped.", systemImage: "exclamationmark.triangle")
                    .font(.system(size: 10)).foregroundStyle(.orange)
            }
            sliderRow("Temperature", value: $model.settings.temperature, range: 0...1, step: 0.05,
                      format: { String(format: "%.2f", $0) })
            stepperRow("Max tokens", value: $model.settings.maxTokens, range: 256...8192, step: 256)
            stepperRow("Server port (0 = auto)", value: $model.settings.mlxPort, range: 0...65535, step: 1)
        }
    }

    // MARK: Overview

    private var overviewSection: some View {
        SettingsGroup("Overview", icon: "text.alignleft") {
            pickerRow("Length", selection: $model.settings.overviewLength, options: OverviewLength.allCases) { $0.label }
            HStack {
                Text("Style").settingLabel()
                Spacer()
                TextField("neutral, informative", text: $model.settings.overviewStyle)
                    .textFieldStyle(.roundedBorder).frame(maxWidth: 260)
            }
            HStack {
                Text("Language (blank = auto)").settingLabel()
                Spacer()
                TextField("en", text: $model.settings.language)
                    .textFieldStyle(.roundedBorder).frame(maxWidth: 120)
            }
        }
    }

    // MARK: Comments

    private var commentsSection: some View {
        SettingsGroup("Comments", icon: "bubble.left.and.bubble.right") {
            Toggle("Include comments", isOn: $model.settings.includeComments)
            if model.settings.includeComments {
                stepperRow("Max comments", value: $model.settings.maxComments, range: 0...200, step: 5)
                pickerRow("Sort", selection: $model.settings.commentSort, options: CommentSort.allCases) { $0.label }
            }
        }
    }

    // MARK: Visual

    private var visualSection: some View {
        SettingsGroup("Visual overview", icon: "photo.stack") {
            Toggle("Include visual overview", isOn: $model.settings.includeVisual)
            if model.settings.includeVisual {
                stepperRow("Max frames", value: $model.settings.maxFrames, range: 0...32, step: 1)
                pickerRow("Frame selection", selection: $model.settings.frameStrategy, options: FrameStrategy.allCases) { $0.label }
            }
        }
    }

    // MARK: Transcript

    private var transcriptSection: some View {
        SettingsGroup("Transcript", icon: "captions.bubble") {
            Toggle("Include transcript", isOn: $model.settings.includeTranscript)
            if model.settings.includeTranscript {
                Toggle("Keep timestamps", isOn: $model.settings.transcriptTimestamps)
            }
        }
    }

    // MARK: Output sections

    private var sectionsSection: some View {
        SettingsGroup("Output sections", icon: "list.bullet.rectangle") {
            Toggle("AI instruction preamble", isOn: $model.settings.sections.aiPreamble)
            Toggle("Video details", isOn: $model.settings.sections.metadata)
            Toggle("Chapters", isOn: $model.settings.sections.chapters)
            Toggle("AI overview", isOn: $model.settings.sections.aiOverview)
            Toggle("Visual overview", isOn: $model.settings.sections.visualOverview)
            Toggle("Comments", isOn: $model.settings.sections.comments)
            Toggle("Transcript", isOn: $model.settings.sections.transcript)
        }
    }

    private var resetRow: some View {
        HStack {
            Spacer()
            Button {
                let model = self.model
                let keepModel = model.settings.model
                model.settings = Settings()
                model.settings.model = keepModel
            } label: {
                Label("Reset to defaults", systemImage: "arrow.counterclockwise")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.glass)
        }
    }

    // MARK: Helpers

    private func label(for m: CachedModel) -> String {
        let name = m.repo.split(separator: "/").last.map(String.init) ?? m.repo
        let badge = m.multimodal ? "👁 " : ""
        return "\(badge)\(name) · \(m.size)"
    }

    @ViewBuilder
    private func sliderRow(_ title: String, value: Binding<Double>, range: ClosedRange<Double>,
                           step: Double, format: @escaping (Double) -> String) -> some View {
        HStack {
            Text(title).settingLabel()
            Slider(value: value, in: range, step: step)
            Text(format(value.wrappedValue))
                .font(.system(size: 11, weight: .semibold).monospacedDigit())
                .foregroundStyle(.secondary).frame(width: 42, alignment: .trailing)
        }
    }

    @ViewBuilder
    private func stepperRow(_ title: String, value: Binding<Int>, range: ClosedRange<Int>, step: Int) -> some View {
        HStack {
            Text(title).settingLabel()
            Spacer()
            Stepper(value: value, in: range, step: step) {
                Text("\(value.wrappedValue)")
                    .font(.system(size: 12, weight: .semibold).monospacedDigit())
                    .frame(minWidth: 40, alignment: .trailing)
            }
        }
    }

    @ViewBuilder
    private func pickerRow<T: Hashable & Identifiable>(_ title: String, selection: Binding<T>,
                                                       options: [T], label: @escaping (T) -> String) -> some View {
        HStack {
            Text(title).settingLabel()
            Spacer()
            Picker("", selection: selection) {
                ForEach(options) { opt in Text(label(opt)).tag(opt) }
            }
            .labelsHidden().frame(maxWidth: 220)
        }
    }
}

/// A titled glass group of controls.
struct SettingsGroup<Content: View>: View {
    var title: String
    var icon: String
    @ViewBuilder var content: () -> Content
    init(_ title: String, icon: String, @ViewBuilder content: @escaping () -> Content) {
        self.title = title; self.icon = icon; self.content = content
    }
    var body: some View {
        GlassCard(corner: 18, padding: 16) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 6) {
                    Image(systemName: icon).font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Theme.accent)
                    Text(title.uppercased())
                        .font(.system(size: 10.5, weight: .bold))
                        .foregroundStyle(.secondary)
                        .kerning(0.6)
                }
                content()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private extension Text {
    func settingLabel() -> some View {
        self.font(.system(size: 12)).foregroundStyle(.primary)
    }
}
