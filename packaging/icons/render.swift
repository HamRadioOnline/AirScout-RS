import Accelerate
import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct IconError: LocalizedError {
    let message: String
    var errorDescription: String? { message }

    init(_ message: String) {
        self.message = message
    }
}

func makeBitmap(size: Int) throws -> CGContext {
    let bitmapInfo = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
    guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
          let context = CGContext(
              data: nil, width: size, height: size, bitsPerComponent: 8,
              bytesPerRow: size * 4, space: colorSpace, bitmapInfo: bitmapInfo
          ) else {
        throw IconError("Unable to allocate \(size)×\(size) RGBA bitmap")
    }

    return context
}

func loadSVG(_ url: URL) throws -> NSImage {
    guard let image = NSImage(contentsOf: url), image.isValid else {
        throw IconError("Unable to load \(url.lastPathComponent) with AppKit SVG support")
    }
    image.cacheMode = .never
    return image
}

func drawSVG(_ image: NSImage, on context: CGContext, in rect: CGRect) {
    NSGraphicsContext.saveGraphicsState()
    defer { NSGraphicsContext.restoreGraphicsState() }
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    image.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1)
}

enum BadgeCorner {
    case bottomRight
    case topLeft
}

struct Badge {
    let logo: NSImage
    let corner: BadgeCorner

    func draw(on context: CGContext) throws {
        let size = context.width
        let width = Int((Double(size) * 0.46).rounded())
        let outline = max(1, Int((Double(size) * 0.01).rounded()))
        let side = width + 2 * outline
        let logoContext = try makeBitmap(size: side)

        // Keep the SVG artwork inside a transparent margin for the outline.
        drawSVG(logo, on: logoContext, in: CGRect(x: outline, y: outline, width: width, height: width))

        let outlineContext = try makeBitmap(size: side)
        guard let image = logoContext.makeImage(),
              let logoPixels = logoContext.data?.assumingMemoryBound(to: UInt8.self),
              let outlinePixels = outlineContext.data?.assumingMemoryBound(to: UInt8.self)
        else {
            throw IconError("Unable to read Rust badge pixels")
        }

        var alpha = (0..<(side * side)).map { logoPixels[$0 * 4 + 3] }
        var dilated = [UInt8](repeating: 0, count: alpha.count)
        let kernelSize = 2 * outline + 1
        // A flat disk takes the maximum alpha within the outline radius.
        let kernel: [UInt8] = (0..<(kernelSize * kernelSize)).map { index in
            let x = index % kernelSize - outline
            let y = index / kernelSize - outline
            return x * x + y * y <= outline * outline ? 0 : 255
        }
        let status = alpha.withUnsafeMutableBytes { source in
            dilated.withUnsafeMutableBytes { destination in
                var input = vImage_Buffer(
                    data: source.baseAddress, height: UInt(side), width: UInt(side), rowBytes: side
                )
                var output = vImage_Buffer(
                    data: destination.baseAddress, height: UInt(side), width: UInt(side), rowBytes: side
                )
                return vImageDilate_Planar8(
                    &input, &output, 0, 0, kernel, UInt(kernelSize), UInt(kernelSize), vImage_Flags(kvImageNoFlags)
                )
            }
        }
        guard status == kvImageNoError else {
            throw IconError("Unable to dilate Rust badge alpha (vImage error \(status))")
        }

        // In premultiplied RGBA, white has each color channel equal to alpha.
        for (index, coverage) in dilated.enumerated() {
            for channel in 0..<4 {
                outlinePixels[index * 4 + channel] = coverage
            }
        }
        guard let rim = outlineContext.makeImage() else {
            throw IconError("Unable to create Rust badge outline")
        }

        let rect = CGRect(
            x: corner == .bottomRight ? size - side : 0,
            y: corner == .topLeft ? size - side : 0,
            width: side, height: side
        )
        context.draw(rim, in: rect)
        context.draw(image, in: rect)
    }
}

func pngData(_ context: CGContext) throws -> Data {
    let data = NSMutableData()
    guard let image = context.makeImage(),
          let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)
    else {
        throw IconError("Unable to create PNG destination")
    }

    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else {
        throw IconError("Unable to encode PNG")
    }
    return data as Data
}

func straightRGBA(_ context: CGContext) throws -> Data {
    guard let pixels = context.data else {
        throw IconError("Unable to read RGBA pixels")
    }

    // CoreGraphics uses premultiplied RGBA. The application API expects
    // straight RGBA, ordered left to right from the top row.
    var rgba = Data(bytes: pixels, count: context.bytesPerRow * context.height)
    for index in stride(from: 0, to: rgba.count, by: 4) {
        let alpha = Int(rgba[index + 3])
        guard alpha > 0 && alpha < 255 else { continue }
        for channel in index..<index + 3 {
            rgba[channel] = UInt8(min(255, (Int(rgba[channel]) * 255 + alpha / 2) / alpha))
        }
    }
    return rgba
}

extension Data {
    mutating func appendUInt32LE(_ value: UInt32) {
        Swift.withUnsafeBytes(of: value.littleEndian) { append(contentsOf: $0) }
    }
}

func dibFrame(_ context: CGContext) throws -> Data {
    let size = context.width
    let rgba = try straightRGBA(context)
    let maskStride = (size + 31) / 32 * 4
    var mask = Data(repeating: 0, count: maskStride * size)

    var dib = Data()
    dib.appendUInt32LE(40) // BITMAPINFOHEADER size
    dib.appendUInt32LE(UInt32(size))
    dib.appendUInt32LE(UInt32(size * 2)) // XOR pixels and AND mask
    dib.appendUInt32LE(0x0020_0001) // 1 plane, 32 bits per pixel
    dib.append(Data(repeating: 0, count: 24)) // BI_RGB and unused metadata

    for row in 0..<size {
        for column in 0..<size {
            let index = ((size - 1 - row) * size + column) * 4
            let alpha = rgba[index + 3]
            dib.append(contentsOf: [rgba[index + 2], rgba[index + 1], rgba[index], alpha])
            if alpha == 0 {
                mask[row * maskStride + column / 8] |= 0x80 >> (column % 8)
            }
        }
    }
    dib.append(mask)
    return dib
}

let iconSizes = [16, 32, 48, 64, 128, 256]

func writeICO(_ frames: [(size: Int, data: Data)], to url: URL) throws {
    var directory = Data([0, 0, 1, 0, UInt8(frames.count), 0])
    var payload = Data()
    for (size, frame) in frames {
        let dimension = UInt8(truncatingIfNeeded: size) // ICO encodes 256px as zero.
        directory.append(contentsOf: [dimension, dimension, 0, 0, 1, 0, 32, 0])
        directory.appendUInt32LE(UInt32(frame.count))
        directory.appendUInt32LE(UInt32(6 + frames.count * 16 + payload.count))
        payload.append(frame)
    }
    directory.append(payload)
    try directory.write(to: url)
}

struct ApplicationIcon {
    let name: String
    let rgbaName: String
    let full: NSImage
    let small: NSImage
    let badge: Badge

    func render(size: Int) throws -> CGContext {
        let context = try makeBitmap(size: size)
        let rect = CGRect(x: 0, y: 0, width: size, height: size)
        if size < 48 {
            drawSVG(small, on: context, in: rect)
        } else {
            drawSVG(full, on: context, in: rect)
            try badge.draw(on: context)
        }
        return context
    }

    func write(to workDir: URL, pngDirectory: String) throws {
        let hicolorDir = workDir.appendingPathComponent(pngDirectory)
        let iconsetDir = workDir.appendingPathComponent("\(name).iconset")
        var pngBySize: [Int: Data] = [:]
        var frames: [(size: Int, data: Data)] = []
        for size in iconSizes {
            let context = try render(size: size)
            let png = try pngData(context)
            pngBySize[size] = png
            try png.write(to: hicolorDir.appendingPathComponent("\(size).png"))
            if size == 32 {
                try straightRGBA(context).write(to: workDir.appendingPathComponent("\(rgbaName).rgba"))
            }
            let frame = size == 256 ? png : try dibFrame(context)
            frames.append((size, frame))
        }
        try writeICO(frames, to: workDir.appendingPathComponent("\(name).ico"))

        for points in [16, 32, 128, 256, 512] {
            for scale in [1, 2] {
                let size = points * scale
                let suffix = scale == 2 ? "@2x" : ""
                let filename = "icon_\(points)x\(points)\(suffix).png"
                let png = try pngBySize[size] ?? pngData(render(size: size))
                pngBySize[size] = png
                try png.write(to: iconsetDir.appendingPathComponent(filename))
            }
        }
    }
}

func generate(sourceDir: URL, workDir: URL) throws {
    let artwork = sourceDir.appendingPathComponent("src")
    let logo = try loadSVG(sourceDir.appendingPathComponent("rust-logo.svg"))

    let main = try ApplicationIcon(
        name: "AirScout-RS",
        rgbaName: "airscout",
        full: loadSVG(artwork.appendingPathComponent("airscout.svg")),
        small: loadSVG(artwork.appendingPathComponent("airscout-small.svg")),
        badge: Badge(logo: logo, corner: .bottomRight)
    )
    let server = try ApplicationIcon(
        name: "AirScout-RS-PlaneServer",
        rgbaName: "airscout-planeserver",
        full: loadSVG(artwork.appendingPathComponent("planeserver.svg")),
        small: loadSVG(artwork.appendingPathComponent("planeserver-small.svg")),
        badge: Badge(logo: logo, corner: .topLeft)
    )

    try main.write(to: workDir, pngDirectory: "hicolor")
    try server.write(to: workDir, pngDirectory: "hicolor-planeserver")
}

do {
    guard CommandLine.arguments.count == 3 else {
        throw IconError("Usage: render.swift <source-dir> <work-dir>")
    }
    try generate(
        sourceDir: URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true),
        workDir: URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
    )
} catch {
    FileHandle.standardError.write(Data("error: \(error.localizedDescription)\n".utf8))
    exit(1)
}
