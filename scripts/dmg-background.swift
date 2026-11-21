// Draws the installer window's background: the brand's dark ink, a mint arrow from where the app
// sits to where Applications sits, and a line about opening an app that isn't notarized.
// Usage: swift scripts/dmg-background.swift <out.png>   (660×400 points, drawn at 2×)
// The icon spots must match make-dmg.sh: not.app at (165, 190), Applications at (495, 190).

import AppKit

let out = CommandLine.arguments.dropFirst().first ?? "background.png"
let size = NSSize(width: 660, height: 400)
let scale: CGFloat = 2

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
)!
rep.size = size
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

func rgb(_ hex: UInt32) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}
let background = rgb(0x181A26)
let mint = rgb(0x8BE9A8)
let ink = rgb(0xECEEF5)
let muted = rgb(0x9CA1B6)

background.setFill()
NSRect(origin: .zero, size: size).fill()

// AppKit's origin is bottom left; Finder places icons from the top left.
func y(_ top: CGFloat) -> CGFloat { size.height - top }

// The arrow, between the two icons (their labels sit below them).
let arrow = NSBezierPath()
arrow.lineWidth = 6
arrow.lineCapStyle = .round
arrow.lineJoinStyle = .round
arrow.move(to: NSPoint(x: 262, y: y(180)))
arrow.line(to: NSPoint(x: 392, y: y(180)))
arrow.move(to: NSPoint(x: 370, y: y(162)))
arrow.line(to: NSPoint(x: 394, y: y(180)))
arrow.line(to: NSPoint(x: 370, y: y(198)))
mint.setStroke()
arrow.stroke()

func text(_ string: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, top: CGFloat) {
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: weight),
        .foregroundColor: color,
        .paragraphStyle: style,
    ]
    let line = NSAttributedString(string: string, attributes: attributes)
    let height = line.size().height
    line.draw(in: NSRect(x: 0, y: y(top) - height, width: 660, height: height))
}

text("Drag not to Applications", size: 20, weight: .semibold, color: ink, top: 40)
text("The first time, right-click not in Applications and choose Open:", size: 12, weight: .regular, color: muted, top: 318)
text("it isn't notarized by Apple, so macOS asks once.", size: 12, weight: .regular, color: muted, top: 336)

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
