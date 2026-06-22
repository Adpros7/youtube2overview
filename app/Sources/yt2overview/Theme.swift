import SwiftUI

/// Centralized palette + glass helpers so the app has one coherent visual language.
enum Theme {
    static let accent = Color(red: 0.98, green: 0.27, blue: 0.30)   // warm coral-red
    static let violet = Color(red: 0.52, green: 0.40, blue: 0.96)
    static let teal = Color(red: 0.26, green: 0.78, blue: 0.79)

    static let cardCorner: CGFloat = 22
    static let controlCorner: CGFloat = 14
}

/// A reusable Liquid Glass surface. On macOS 26 this uses the native `.glassEffect`;
/// the modifier degrades gracefully if unavailable.
struct GlassCard<Content: View>: View {
    var corner: CGFloat = Theme.cardCorner
    var padding: CGFloat = 20
    @ViewBuilder var content: () -> Content

    var body: some View {
        content()
            .padding(padding)
            .glassEffect(
                .regular.interactive(),
                in: .rect(cornerRadius: corner)
            )
            .overlay(
                RoundedRectangle(cornerRadius: corner)
                    .strokeBorder(.white.opacity(0.12), lineWidth: 1)
            )
    }
}

extension View {
    /// Apply a glass surface inline.
    func glassPanel(corner: CGFloat = Theme.controlCorner) -> some View {
        self.glassEffect(.regular.interactive(), in: .rect(cornerRadius: corner))
    }
}
