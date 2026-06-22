import SwiftUI

/// A macOS "mica"-style backdrop: a translucent material that samples the desktop,
/// warmed with soft color blooms so glass controls read against it.
struct MicaBackground: View {
    @State private var drift = false

    var body: some View {
        ZStack {
            VisualEffectView(material: .underWindowBackground, blending: .behindWindow)

            // Crisp, bright color orbs that slowly drift. Kept sharp (low blur) on
            // purpose: the glass surfaces above need high-frequency content with
            // real edges to bend, or the native lensing/refraction is invisible.
            GeometryReader { geo in
                ZStack {
                    bloom(Theme.accent.opacity(0.85), 0.50)
                        .offset(x: drift ? -geo.size.width * 0.20 : -geo.size.width * 0.32,
                                y: drift ? -geo.size.height * 0.18 : -geo.size.height * 0.30)
                    bloom(Theme.violet.opacity(0.80), 0.58)
                        .offset(x: drift ? geo.size.width * 0.30 : geo.size.width * 0.40,
                                y: drift ? geo.size.height * 0.06 : geo.size.height * 0.24)
                    bloom(Theme.teal.opacity(0.70), 0.42)
                        .offset(x: drift ? geo.size.width * 0.12 : -geo.size.width * 0.02,
                                y: drift ? geo.size.height * 0.44 : geo.size.height * 0.34)
                    // Small bright accent so glass edges show a visible lens.
                    bloom(.white.opacity(0.35), 0.20)
                        .offset(x: drift ? geo.size.width * 0.18 : geo.size.width * 0.05,
                                y: drift ? -geo.size.height * 0.02 : geo.size.height * 0.12)
                }
                .frame(width: geo.size.width, height: geo.size.height)
                .blur(radius: 24)
            }
            .opacity(0.95)

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
