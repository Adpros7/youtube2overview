import SwiftUI

/// A browsable list of past generations; selecting one reloads it into the main view.
struct HistoryView: View {
    @Bindable var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.15)
            if model.history.entries.isEmpty {
                empty
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(model.history.entries) { entry in
                            HistoryRow(entry: entry,
                                       load: { model.load(entry) },
                                       delete: { model.history.delete(entry) })
                        }
                    }
                    .padding(16)
                }
            }
        }
        .frame(width: 460, height: 560)
        .background(MicaBackground().ignoresSafeArea())
    }

    private var header: some View {
        HStack {
            Image(systemName: "clock.arrow.circlepath")
            Text("History").font(.system(size: 15, weight: .bold))
            Spacer()
            if !model.history.entries.isEmpty {
                Button(role: .destructive) {
                    model.history.clear()
                } label: {
                    Label("Clear", systemImage: "trash")
                        .font(.system(size: 12, weight: .medium))
                }
                .buttonStyle(.glass)
            }
            Button("Done") { dismiss() }
                .buttonStyle(.glassProminent).tint(Theme.accent)
        }
        .padding(16)
    }

    private var empty: some View {
        VStack(spacing: 8) {
            Spacer()
            Image(systemName: "clock").font(.system(size: 30)).foregroundStyle(.secondary)
            Text("No history yet").font(.system(size: 14, weight: .medium))
            Text("Overviews you generate are saved here.")
                .font(.system(size: 11)).foregroundStyle(.secondary)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct HistoryRow: View {
    var entry: HistoryEntry
    var load: () -> Void
    var delete: () -> Void
    @State private var hovering = false

    var body: some View {
        GlassCard(corner: 14, padding: 12) {
            HStack(spacing: 12) {
                Button(action: load) {
                    HStack(spacing: 12) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 8)
                                .fill(LinearGradient(colors: [Theme.accent.opacity(0.8), Theme.violet.opacity(0.8)],
                                                     startPoint: .topLeading, endPoint: .bottomTrailing))
                                .frame(width: 30, height: 30)
                            Image(systemName: "play.fill").font(.system(size: 12)).foregroundStyle(.white)
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry.title).font(.system(size: 13, weight: .semibold))
                                .lineLimit(1).foregroundStyle(.primary)
                            HStack(spacing: 6) {
                                if !entry.channel.isEmpty {
                                    Text(entry.channel)
                                    Text("·")
                                }
                                Text(entry.date, format: .relative(presentation: .named))
                            }
                            .font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(1)
                        }
                        Spacer()
                    }
                }
                .buttonStyle(.plain)

                Button(action: delete) {
                    Image(systemName: "trash").font(.system(size: 12))
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .opacity(hovering ? 1 : 0.35)
            }
        }
        .onHover { hovering = $0 }
    }
}
