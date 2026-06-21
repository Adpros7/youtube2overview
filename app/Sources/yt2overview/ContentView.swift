import SwiftUI

struct ContentView: View {
    @State private var url: String = ""

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.15)
            ScrollView {
                VStack(spacing: 20) {
                    inputCard
                    placeholder
                }
                .padding(28)
                .frame(maxWidth: 880)
                .frame(maxWidth: .infinity)
            }
        }
    }

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
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .padding(.top, 18) // clear the hidden titlebar
    }

    private var inputCard: some View {
        GlassCard {
            VStack(alignment: .leading, spacing: 14) {
                Text("Paste a YouTube link")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    Image(systemName: "link")
                        .foregroundStyle(.secondary)
                    TextField("https://youtube.com/watch?v=…", text: $url)
                        .textFieldStyle(.plain)
                        .font(.system(size: 15))
                    Button {
                        // wired in Phase 8
                    } label: {
                        Label("Generate", systemImage: "sparkles")
                            .font(.system(size: 14, weight: .semibold))
                    }
                    .buttonStyle(.glassProminent)
                    .tint(Theme.accent)
                }
                .padding(12)
                .glassPanel()
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
