import SwiftUI

/// Real macOS vibrancy: a transparent window backdrop where the OS samples and
/// blurs the actual desktop behind the window (`.behindWindow` blending). The glass
/// surfaces above then refract genuine desktop content — not painted-on blobs.
struct MicaBackground: View {
    var body: some View {
        // `.behindWindow` makes the material sample the live desktop; the glass
        // cards over it lens that real content, which is the actual refraction.
        // A faint warm tint keeps the app's identity without hiding the desktop.
        VisualEffectView(material: .fullScreenUI, blending: .behindWindow)
            .overlay(Theme.accent.opacity(0.06))
            .overlay(
                LinearGradient(
                    colors: [.clear, .black.opacity(0.10)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
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
