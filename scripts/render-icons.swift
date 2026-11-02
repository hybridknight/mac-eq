// SVG files in assets/ are the editable source of truth. AppKit rasterizes them
// at each native icon size, preserving transparency. No external packages.
import AppKit
import Foundation
let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let output = root.appendingPathComponent("bundle/generated", isDirectory: true)
let iconset = output.appendingPathComponent("MacEQ.iconset", isDirectory: true)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
func image(_ name: String) -> NSImage {
    guard let image = NSImage(contentsOf: root.appendingPathComponent("assets/\(name).svg")) else {
        fatalError("Cannot read assets/\(name).svg")
    }
    return image
}
func render(_ image: NSImage, width: Int, height: Int, to destination: URL) throws {
    guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { fatalError("Cannot create bitmap") }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    NSGraphicsContext.current?.imageInterpolation = .high
    image.draw(in: NSRect(x: 0, y: 0, width: width, height: height), from: .zero,
        operation: .copy, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    guard let data = bitmap.representation(using: .png, properties: [:]) else { fatalError("Cannot encode PNG") }
    try data.write(to: destination, options: .atomic)
}
let app = image("AppIcon")
for size in [16, 32, 128, 256, 512] {
    try render(app, width: size, height: size, to: iconset.appendingPathComponent("icon_\(size)x\(size).png"))
    try render(app, width: size * 2, height: size * 2, to: iconset.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
}
try render(app, width: 1024, height: 1024, to: output.appendingPathComponent("AppIcon.png"))
try render(app, width: 256, height: 256, to: output.appendingPathComponent("AppIcon256.png"))
let menu = image("MenuBarTemplate")
try render(menu, width: 18, height: 18, to: output.appendingPathComponent("MenuBarTemplate.png"))
try render(menu, width: 36, height: 36, to: output.appendingPathComponent("MenuBarTemplate@2x.png"))
try render(image("Artwork"), width: 1200, height: 1000, to: output.appendingPathComponent("Artwork.png"))
print("Rendered MacEQ SVG artwork and icon sizes")
