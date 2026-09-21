// Draws pitboard's icon: the limits it exists to show, as three bars on a dark board.
// Run by scripts/build-app.sh, so the icon is built rather than committed.
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let iconset = "\(out)/AppIcon.iconset"
try? FileManager.default.createDirectory(
    atPath: iconset, withIntermediateDirectories: true)

/// Everything is drawn in a 1024 square and scaled, so one drawing serves every size.
func draw(_ side: CGFloat, into context: CGContext) {
    let scale = side / 1024
    context.scaleBy(x: scale, y: scale)

    // The board: a dark rounded square, lighter at the top as macOS icons are.
    let board = CGRect(x: 64, y: 64, width: 896, height: 896)
    let shape = CGPath(
        roundedRect: board, cornerWidth: 200, cornerHeight: 200, transform: nil)
    context.saveGState()
    context.addPath(shape)
    context.clip()
    let colours =
        [
            CGColor(red: 0.16, green: 0.18, blue: 0.22, alpha: 1),
            CGColor(red: 0.06, green: 0.07, blue: 0.09, alpha: 1),
        ] as CFArray
    let gradient = CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colours, locations: [0, 1])!
    context.drawLinearGradient(
        gradient, start: CGPoint(x: 0, y: 960), end: CGPoint(x: 0, y: 64), options: [])

    // Three limits, each with the room it has left behind it.
    let bars: [(y: CGFloat, filled: CGFloat)] = [(660, 0.42), (492, 0.74), (324, 0.95)]
    let left = CGFloat(208)
    let width = CGFloat(608)
    let height = CGFloat(84)
    for bar in bars {
        let track = CGRect(x: left, y: bar.y, width: width, height: height)
        context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.12))
        context.addPath(
            CGPath(
                roundedRect: track, cornerWidth: height / 2, cornerHeight: height / 2,
                transform: nil))
        context.fillPath()

        let used = CGRect(x: left, y: bar.y, width: width * bar.filled, height: height)
        context.setFillColor(CGColor(red: 0.30, green: 0.82, blue: 0.44, alpha: 1))
        context.addPath(
            CGPath(
                roundedRect: used, cornerWidth: height / 2, cornerHeight: height / 2,
                transform: nil))
        context.fillPath()
    }
    context.restoreGState()
}

for side in [16, 32, 64, 128, 256, 512, 1024] {
    let context = CGContext(
        data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    draw(CGFloat(side), into: context)
    let image = NSBitmapImageRep(cgImage: context.makeImage()!)
    let png = image.representation(using: .png, properties: [:])!
    // Every size is written under the name it has, and under the name it doubles when
    // there is a half of it to double; 8x8 is not a size macOS asks for.
    var names = ["icon_\(side)x\(side).png"]
    if side >= 32 { names.append("icon_\(side / 2)x\(side / 2)@2x.png") }
    for name in names {
        try? png.write(to: URL(fileURLWithPath: "\(iconset)/\(name)"))
    }
}
