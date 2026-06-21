import SwiftUI

/// A macOS "mica"-style backdrop: a translucent material that samples the desktop,
/// warmed with soft color blooms so glass controls read against it.
struct MicaBackground: View {
    @State private var drift = false

    var body: some View {
        ZStack {
            VisualEffectView(material: .underWindowBackground, blending: .behindWindow)

            // Color blooms that slowly drift, giving the mica its depth + tint.
            GeometryReader { geo in
                ZStack {
                    bloom(Theme.accent.opacity(0.45), 0.62)
                        .offset(x: drift ? -geo.size.width * 0.22 : -geo.size.width * 0.30,
                                y: drift ? -geo.size.height * 0.20 : -geo.size.height * 0.28)
                    bloom(Theme.violet.opacity(0.40), 0.70)
                        .offset(x: geo.size.width * 0.34,
                                y: drift ? geo.size.height * 0.10 : geo.size.height * 0.22)
                    bloom(Theme.teal.opacity(0.30), 0.55)
                        .offset(x: drift ? geo.size.width * 0.10 : geo.size.width * 0.02,
                                y: geo.size.height * 0.40)
                }
                .frame(width: geo.size.width, height: geo.size.height)
                .blur(radius: 80)
            }
            .opacity(0.9)

            // Subtle top-down darkening keeps content legible.
            LinearGradient(
                colors: [.black.opacity(0.05), .clear, .black.opacity(0.12)],
                startPoint: .top,
                endPoint: .bottom
            )
        }
        .onAppear {
            withAnimation(.easeInOut(duration: 18).repeatForever(autoreverses: true)) {
                drift = true
            }
        }
    }

    private func bloom(_ color: Color, _ scale: CGFloat) -> some View {
        Circle()
            .fill(color)
            .frame(width: 520 * scale, height: 520 * scale)
    }
}

/// Bridges `NSVisualEffectView` so we get genuine macOS vibrancy/material.
struct VisualEffectView: NSViewRepresentable {
    var material: NSVisualEffectView.Material
    var blending: NSVisualEffectView.BlendingMode

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blending
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        nsView.material = material
        nsView.blendingMode = blending
    }
}
