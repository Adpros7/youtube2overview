// Generates AppIcon.icns: a Liquid-Glass-ish rounded square with a play glyph.
// Usage: swift make_icon.swift <output_dir>
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let iconset = (outDir as NSString).appendingPathComponent("AppIcon.iconset")
try? FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)

func render(_ size: Int) -> Data {
    let s = CGFloat(size)
    let image = NSImage(size: NSSize(width: s, height: s))
    image.lockFocus()
    let ctx = NSGraphicsContext.current!.cgContext

    // Rounded-rect background gradient.
    let inset = s * 0.06
    let rect = CGRect(x: inset, y: inset, width: s - inset * 2, height: s - inset * 2)
    let path = NSBezierPath(roundedRect: rect, xRadius: s * 0.22, yRadius: s * 0.22)
    path.addClip()
    let colors = [
        NSColor(calibratedRed: 0.98, green: 0.27, blue: 0.30, alpha: 1).cgColor,
        NSColor(calibratedRed: 0.52, green: 0.40, blue: 0.96, alpha: 1).cgColor,
    ] as CFArray
    let grad = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors,
                          locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: s), end: CGPoint(x: s, y: 0), options: [])

    // Soft glass highlight.
    NSColor.white.withAlphaComponent(0.18).setFill()
    NSBezierPath(ovalIn: CGRect(x: -s * 0.2, y: s * 0.45, width: s * 0.9, height: s * 0.9)).fill()

    // Play triangle.
    let t = NSBezierPath()
    let cx = s * 0.5, cy = s * 0.5, r = s * 0.18
    t.move(to: NSPoint(x: cx - r * 0.8, y: cy - r))
    t.line(to: NSPoint(x: cx - r * 0.8, y: cy + r))
    t.line(to: NSPoint(x: cx + r, y: cy))
    t.close()
    NSColor.white.setFill()
    t.fill()

    image.unlockFocus()
    let tiff = image.tiffRepresentation!
    let rep = NSBitmapImageRep(data: tiff)!
    return rep.representation(using: .png, properties: [:])!
}

let sizes = [16, 32, 64, 128, 256, 512, 1024]
for sz in sizes {
    let data = render(sz)
    let base = sz == 1024 ? "icon_512x512@2x" : "icon_\(sz)x\(sz)"
    try? data.write(to: URL(fileURLWithPath: (iconset as NSString).appendingPathComponent("\(base).png")))
    // @2x variants for the standard sizes.
    if sz <= 512 {
        let data2 = render(sz * 2)
        try? data2.write(to: URL(fileURLWithPath: (iconset as NSString).appendingPathComponent("icon_\(sz)x\(sz)@2x.png")))
    }
}
print(iconset)
