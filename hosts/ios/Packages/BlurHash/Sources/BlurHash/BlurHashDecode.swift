// Based on Wolt's blurhash reference implementation.
//
// MIT License
//
// Copyright (c) 2018 Wolt Enterprises
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

import UIKit

public extension UIImage {
    convenience init?(blurHash: BlurHash, size: CGSize, punch: Float = 1) {
        let value = blurHash.value

        let sizeFlag = String(value[0]).decode83()
        let numY = (sizeFlag / 9) + 1
        let numX = (sizeFlag % 9) + 1

        let quantisedMaximumValue = String(value[1]).decode83()
        let maximumValue = Float(quantisedMaximumValue + 1) / 166

        let colours = decodeColors(
            from: value,
            count: numX * numY,
            maximumValue: maximumValue * punch
        )

        let width = Int(size.width)
        let height = Int(size.height)
        let bytesPerRow = width * 3
        guard let data = CFDataCreateMutable(kCFAllocatorDefault, bytesPerRow * height) else { return nil }
        CFDataSetLength(data, bytesPerRow * height)
        guard let pixels = CFDataGetMutableBytePtr(data) else { return nil }

        for pixelY in 0 ..< height {
            for pixelX in 0 ..< width {
                var red: Float = 0
                var green: Float = 0
                var blue: Float = 0

                for componentY in 0 ..< numY {
                    for componentX in 0 ..< numX {
                        let basis = cos(Float.pi * Float(pixelX) * Float(componentX) / Float(width)) *
                            cos(Float.pi * Float(pixelY) * Float(componentY) / Float(height))
                        let colour = colours[componentX + componentY * numX]
                        red += colour.red * basis
                        green += colour.green * basis
                        blue += colour.blue * basis
                    }
                }

                let intR = UInt8(BlurHashMath.linearToSRGB(red))
                let intG = UInt8(BlurHashMath.linearToSRGB(green))
                let intB = UInt8(BlurHashMath.linearToSRGB(blue))

                pixels[3 * pixelX + 0 + pixelY * bytesPerRow] = intR
                pixels[3 * pixelX + 1 + pixelY * bytesPerRow] = intG
                pixels[3 * pixelX + 2 + pixelY * bytesPerRow] = intB
            }
        }

        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue)

        guard let provider = CGDataProvider(data: data) else { return nil }
        guard let cgImage = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 24,
            bytesPerRow: bytesPerRow,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: true,
            intent: .defaultIntent
        ) else { return nil }

        self.init(cgImage: cgImage)
    }
}

private func decodeColors(from blurHash: String, count: Int, maximumValue: Float) -> [BlurHashColor] {
    (0 ..< count).map { index in
        if index == 0 {
            let value = String(blurHash[2 ..< 6]).decode83()
            return decodeDC(value)
        }

        let value = String(blurHash[4 + index * 2 ..< 4 + index * 2 + 2]).decode83()
        return decodeAC(value, maximumValue: maximumValue)
    }
}

private func decodeDC(_ value: Int) -> BlurHashColor {
    let intR = value >> 16
    let intG = (value >> 8) & 255
    let intB = value & 255
    return BlurHashColor(
        red: BlurHashMath.sRGBToLinear(intR),
        green: BlurHashMath.sRGBToLinear(intG),
        blue: BlurHashMath.sRGBToLinear(intB)
    )
}

private func decodeAC(_ value: Int, maximumValue: Float) -> BlurHashColor {
    let quantR = value / (19 * 19)
    let quantG = (value / 19) % 19
    let quantB = value % 19

    return BlurHashColor(
        red: BlurHashMath.signPow((Float(quantR) - 9) / 9, 2) * maximumValue,
        green: BlurHashMath.signPow((Float(quantG) - 9) / 9, 2) * maximumValue,
        blue: BlurHashMath.signPow((Float(quantB) - 9) / 9, 2) * maximumValue
    )
}

private extension String {
    func decode83() -> Int {
        var value = 0
        for character in self {
            if let digit = BlurHashMath.decodeCharacters[character] {
                value = value * 83 + digit
            }
        }
        return value
    }

    subscript(offset: Int) -> Character {
        self[index(startIndex, offsetBy: offset)]
    }

    subscript(bounds: CountableClosedRange<Int>) -> Substring {
        let start = index(startIndex, offsetBy: bounds.lowerBound)
        let end = index(startIndex, offsetBy: bounds.upperBound)
        return self[start ... end]
    }

    subscript(bounds: CountableRange<Int>) -> Substring {
        let start = index(startIndex, offsetBy: bounds.lowerBound)
        let end = index(startIndex, offsetBy: bounds.upperBound)
        return self[start ..< end]
    }
}
