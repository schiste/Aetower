#!/usr/bin/env swift
import AppKit

let outputPath = CommandLine.arguments.dropFirst().first ?? "aetower-dmg-background.png"
let version = CommandLine.arguments.dropFirst(2).first ?? ""
let build = CommandLine.arguments.dropFirst(3).first ?? ""

let width = 720
let height = 440
let size = NSSize(width: width, height: height)

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: width,
    pixelsHigh: height,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
) else {
    fputs("failed to create bitmap context\n", stderr)
    exit(1)
}

func color(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(deviceRed: red / 255, green: green / 255, blue: blue / 255, alpha: alpha)
}

func drawText(_ text: String, in rect: NSRect, font: NSFont, color textColor: NSColor, alignment: NSTextAlignment = .center, lineHeight: CGFloat? = nil) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = alignment
    if let lineHeight {
        paragraph.minimumLineHeight = lineHeight
        paragraph.maximumLineHeight = lineHeight
    }

    let attributed = NSAttributedString(
        string: text,
        attributes: [
            .font: font,
            .foregroundColor: textColor,
            .paragraphStyle: paragraph,
            .kern: -0.25,
        ]
    )
    attributed.draw(in: rect)
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

guard let context = NSGraphicsContext.current?.cgContext else {
    fputs("failed to create graphics context\n", stderr)
    exit(1)
}

context.setAllowsAntialiasing(true)
context.setShouldAntialias(true)

let bounds = NSRect(origin: .zero, size: size)

let baseGradient = NSGradient(colors: [
    color(248, 251, 249),
    color(236, 246, 241),
    color(250, 247, 255),
])!
baseGradient.draw(in: bounds, angle: -28)

for index in 0..<18 {
    let x = CGFloat((index * 67) % width) - 80
    let y = CGFloat((index * 41) % height) - 80
    let alpha = CGFloat(0.06 + Double(index % 4) * 0.018)
    let path = NSBezierPath(ovalIn: NSRect(x: x, y: y, width: 180, height: 180))
    color(index % 2 == 0 ? 54 : 123, index % 2 == 0 ? 214 : 63, index % 2 == 0 ? 131 : 242, alpha).setFill()
    path.fill()
}

let wavePath = NSBezierPath()
wavePath.move(to: NSPoint(x: 42, y: 165))
wavePath.curve(to: NSPoint(x: 190, y: 158), controlPoint1: NSPoint(x: 86, y: 96), controlPoint2: NSPoint(x: 142, y: 101))
wavePath.curve(to: NSPoint(x: 342, y: 172), controlPoint1: NSPoint(x: 238, y: 215), controlPoint2: NSPoint(x: 287, y: 238))
wavePath.curve(to: NSPoint(x: 520, y: 166), controlPoint1: NSPoint(x: 404, y: 98), controlPoint2: NSPoint(x: 461, y: 107))
wavePath.curve(to: NSPoint(x: 678, y: 164), controlPoint1: NSPoint(x: 574, y: 224), controlPoint2: NSPoint(x: 625, y: 221))
wavePath.line(to: NSPoint(x: 678, y: 52))
wavePath.curve(to: NSPoint(x: 517, y: 80), controlPoint1: NSPoint(x: 628, y: 54), controlPoint2: NSPoint(x: 575, y: 36))
wavePath.curve(to: NSPoint(x: 342, y: 76), controlPoint1: NSPoint(x: 458, y: 124), controlPoint2: NSPoint(x: 403, y: 139))
wavePath.curve(to: NSPoint(x: 184, y: 80), controlPoint1: NSPoint(x: 281, y: 14), controlPoint2: NSPoint(x: 235, y: 34))
wavePath.curve(to: NSPoint(x: 42, y: 66), controlPoint1: NSPoint(x: 137, y: 124), controlPoint2: NSPoint(x: 91, y: 94))
wavePath.close()
color(123, 63, 242, 0.20).setFill()
wavePath.fill()

let accentPath = NSBezierPath()
accentPath.lineWidth = 8
accentPath.move(to: NSPoint(x: 78, y: 162))
accentPath.curve(to: NSPoint(x: 214, y: 154), controlPoint1: NSPoint(x: 125, y: 124), controlPoint2: NSPoint(x: 169, y: 121))
accentPath.curve(to: NSPoint(x: 352, y: 170), controlPoint1: NSPoint(x: 259, y: 188), controlPoint2: NSPoint(x: 302, y: 220))
accentPath.curve(to: NSPoint(x: 498, y: 159), controlPoint1: NSPoint(x: 397, y: 122), controlPoint2: NSPoint(x: 447, y: 113))
accentPath.curve(to: NSPoint(x: 642, y: 160), controlPoint1: NSPoint(x: 545, y: 199), controlPoint2: NSPoint(x: 595, y: 199))
color(54, 214, 131, 0.85).setStroke()
accentPath.stroke()

let baseline = NSBezierPath()
baseline.lineWidth = 1.5
baseline.move(to: NSPoint(x: 60, y: 164))
baseline.line(to: NSPoint(x: 660, y: 164))
color(16, 23, 22, 0.20).setStroke()
baseline.stroke()

let titleFont = NSFont.systemFont(ofSize: 34, weight: .bold)
let bodyFont = NSFont.systemFont(ofSize: 16, weight: .medium)
let labelFont = NSFont.systemFont(ofSize: 13, weight: .semibold)
let versionFont = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)

drawText(
    "Drag Aetower to Applications",
    in: NSRect(x: 90, y: 350, width: 540, height: 44),
    font: titleFont,
    color: color(16, 23, 22)
)

drawText(
    "Give your Mac a tower with a view.",
    in: NSRect(x: 140, y: 320, width: 440, height: 26),
    font: bodyFont,
    color: color(84, 98, 96)
)

drawText(
    "Aetower",
    in: NSRect(x: 102, y: 64, width: 180, height: 22),
    font: labelFont,
    color: color(16, 23, 22, 0.72)
)
drawText(
    "Applications",
    in: NSRect(x: 438, y: 64, width: 180, height: 22),
    font: labelFont,
    color: color(16, 23, 22, 0.72)
)

let arrowPath = NSBezierPath()
arrowPath.lineWidth = 3
arrowPath.move(to: NSPoint(x: 292, y: 230))
arrowPath.line(to: NSPoint(x: 428, y: 230))
arrowPath.move(to: NSPoint(x: 410, y: 244))
arrowPath.line(to: NSPoint(x: 430, y: 230))
arrowPath.line(to: NSPoint(x: 410, y: 216))
color(16, 23, 22, 0.46).setStroke()
arrowPath.stroke()

if !version.isEmpty {
    let versionLine = build.isEmpty ? "Developer Preview \(version)" : "Developer Preview \(version) build \(build)"
    drawText(
        versionLine,
        in: NSRect(x: 0, y: 22, width: CGFloat(width), height: 18),
        font: versionFont,
        color: color(84, 98, 96, 0.70)
    )
}

NSGraphicsContext.restoreGraphicsState()

guard let pngData = rep.representation(using: .png, properties: [:]) else {
    fputs("failed to encode png\n", stderr)
    exit(1)
}

do {
    try pngData.write(to: URL(fileURLWithPath: outputPath), options: .atomic)
} catch {
    fputs("failed to write \(outputPath): \(error)\n", stderr)
    exit(1)
}
