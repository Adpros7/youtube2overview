import SwiftUI
import AppKit

enum Clipboard {
    static func copy(_ text: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
    }
}

struct ResultsView: View {
    var model: AppModel
    var result: JobResult

    var body: some View {
        VStack(spacing: 18) {
            videoHeader
            modeBar
            // The composed document for the selected mode.
            GlassCard {
                Text(model.currentOutput())
                    .font(.system(size: 12.5))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            // Per-section copy.
            ForEach(result.outputs.sections) { section in
                SectionCard(section: section)
            }
        }
    }

    private var videoHeader: some View {
        GlassCard {
            HStack(alignment: .top, spacing: 14) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(result.data.meta.title)
                        .font(.system(size: 17, weight: .bold))
                        .lineLimit(2)
                    HStack(spacing: 10) {
                        Label(result.data.meta.channel, systemImage: "person.crop.circle")
                        if result.data.frameCount > 0 {
                            Label("\(result.data.frameCount) frames", systemImage: "photo.stack")
                        }
                        if !result.data.modelUsed.isEmpty {
                            Label(shortModel(result.data.modelUsed), systemImage: "cpu")
                        }
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                }
                Spacer()
            }
        }
    }

    private var modeBar: some View {
        HStack(spacing: 12) {
            Picker("", selection: Binding(get: { model.outputMode }, set: { model.outputMode = $0 })) {
                Text("Readable").tag("human")
                Text("AI-optimized").tag("ai")
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 240)

            Spacer()

            CopyButton(label: "Copy all", text: model.currentOutput(), prominent: true)
        }
    }

    private func shortModel(_ m: String) -> String {
        m.split(separator: "/").last.map(String.init) ?? m
    }
}

struct SectionCard: View {
    var section: OutputSection
    @State private var expanded = true

    var body: some View {
        GlassCard {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Button {
                        withAnimation(.easeInOut(duration: 0.18)) { expanded.toggle() }
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: expanded ? "chevron.down" : "chevron.right")
                                .font(.system(size: 10, weight: .bold))
                            Text(section.title).font(.system(size: 13, weight: .semibold))
                        }
                    }
                    .buttonStyle(.plain)
                    Spacer()
                    CopyButton(label: "Copy", text: section.markdown, prominent: false)
                }
                if expanded {
                    Text(section.markdown)
                        .font(.system(size: 12))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }
}

struct CopyButton: View {
    var label: String
    var text: String
    var prominent: Bool
    @State private var copied = false

    var body: some View {
        let button = Button {
            Clipboard.copy(text)
            withAnimation { copied = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
                withAnimation { copied = false }
            }
        } label: {
            Label(copied ? "Copied" : label, systemImage: copied ? "checkmark" : "doc.on.doc")
                .font(.system(size: 12, weight: .semibold))
        }

        Group {
            if prominent {
                button.buttonStyle(.glassProminent).tint(Theme.accent)
            } else {
                button.buttonStyle(.glass)
            }
        }
    }
}
