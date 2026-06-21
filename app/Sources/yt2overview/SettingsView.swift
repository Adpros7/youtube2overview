import SwiftUI

/// Granular controls. (Expanded in Phase 9.)
struct SettingsView: View {
    @Bindable var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Settings").font(.system(size: 15, weight: .bold))
                Spacer()
                Button("Done") { dismiss() }.buttonStyle(.glassProminent).tint(Theme.accent)
            }
            .padding(16)
            Divider().opacity(0.15)
            ScrollView {
                Text("Controls coming online…")
                    .foregroundStyle(.secondary)
                    .padding(40)
            }
        }
        .frame(width: 480, height: 560)
        .background(MicaBackground().ignoresSafeArea())
    }
}
