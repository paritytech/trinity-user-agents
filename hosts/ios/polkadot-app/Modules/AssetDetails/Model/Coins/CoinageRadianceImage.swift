import Foundation

/// Reads a Radiance `.hdr` face of the prefiltered studio environment.
///
/// The studio ships as HDR rather than as an ordinary image because it holds light, not colour: the
/// bright parts of a softbox run well past 1 and are what a polished coin actually reflects. There
/// is no system decoder for the format, so this is the reference host's reader, which handles the
/// run-length encoded scanlines the exporter writes.
enum CoinageRadianceImage {
    struct Face {
        let width: Int
        let height: Int
        /// RGBA, alpha always 1, ready to upload to an `rgba16Float` texture.
        let pixels: [Float16]
    }

    enum Failure: Error {
        case malformed(String)
    }

    static func decode(_ data: Data) throws -> Face {
        let bytes = [UInt8](data)
        var cursor = 0

        func line() throws -> String {
            var text = ""

            while cursor < bytes.count, bytes[cursor] != 10 {
                text.append(Character(UnicodeScalar(bytes[cursor])))
                cursor += 1
            }

            guard cursor < bytes.count else { throw Failure.malformed("unterminated header") }

            cursor += 1

            return text
        }

        while try !line().isEmpty {}

        let dimensions = try line().split(separator: " ")

        guard dimensions.count >= 4,
              let height = Int(dimensions[1]),
              let width = Int(dimensions[3])
        else {
            throw Failure.malformed("no resolution line")
        }

        var pixels = [Float16](repeating: 0, count: width * height * 4)
        var scanline = [UInt8](repeating: 0, count: width * 4)

        for line in 0 ..< height {
            try read(bytes, &cursor, into: &scanline, width: width)
            convert(scanline, into: &pixels, atLine: line, width: width)
        }

        return Face(width: width, height: height, pixels: pixels)
    }
}

private extension CoinageRadianceImage {
    /// Adaptive run-length encoding, one colour channel at a time across the row.
    static func read(
        _ bytes: [UInt8],
        _ cursor: inout Int,
        into scanline: inout [UInt8],
        width: Int
    ) throws {
        guard cursor + 4 <= bytes.count else { throw Failure.malformed("truncated scanline") }

        guard width >= 8, width < 32_768, bytes[cursor] == 2, bytes[cursor + 1] == 2 else {
            guard cursor + width * 4 <= bytes.count else { throw Failure.malformed("truncated row") }

            for index in 0 ..< width * 4 {
                scanline[index] = bytes[cursor + index]
            }
            cursor += width * 4

            return
        }

        cursor += 4

        for channel in 0 ..< 4 {
            try readChannel(bytes, &cursor, into: &scanline, width: width, channel: channel)
        }
    }

    static func readChannel(
        _ bytes: [UInt8],
        _ cursor: inout Int,
        into scanline: inout [UInt8],
        width: Int,
        channel: Int
    ) throws {
        var column = 0

        while column < width {
            guard cursor < bytes.count else { throw Failure.malformed("truncated run") }

            var count = Int(bytes[cursor])
            cursor += 1
            let isRun = count > 128

            if isRun { count -= 128 }

            guard !isRun || cursor < bytes.count else { throw Failure.malformed("truncated run value") }

            let repeated = isRun ? bytes[cursor] : 0

            if isRun { cursor += 1 }

            for _ in 0 ..< count where column < width {
                guard isRun || cursor < bytes.count else { throw Failure.malformed("truncated literal") }

                scanline[column * 4 + channel] = isRun ? repeated : bytes[cursor]

                if !isRun { cursor += 1 }

                column += 1
            }
        }
    }

    /// Radiance packs a shared exponent in the alpha byte: the mantissa bytes are scaled by
    /// `2^(exponent - 136)`, the half-step keeping the quantisation unbiased.
    static func convert(
        _ scanline: [UInt8],
        into pixels: inout [Float16],
        atLine line: Int,
        width: Int
    ) {
        for column in 0 ..< width {
            let exponent = Int(scanline[column * 4 + 3])
            let scale: Float = exponent > 0 ? powf(2, Float(exponent - 136)) : 0
            let offset = (line * width + column) * 4

            for channel in 0 ..< 3 {
                pixels[offset + channel] = Float16(
                    exponent > 0 ? (Float(scanline[column * 4 + channel]) + 0.5) * scale : 0
                )
            }

            pixels[offset + 3] = 1
        }
    }
}
