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
    func blurHash(numberOfComponents components: (Int, Int)) -> String? {
        let pixelWidth = Int(round(size.width * scale))
        let pixelHeight = Int(round(size.height * scale))

        guard pixelWidth > 0,
              pixelHeight > 0,
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                  data: nil,
                  width: pixelWidth,
                  height: pixelHeight,
                  bitsPerComponent: 8,
                  bytesPerRow: pixelWidth * 4,
                  space: colorSpace,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ) else { return nil }
        context.scaleBy(x: scale, y: -scale)
        context.translateBy(x: 0, y: -size.height)

        UIGraphicsPushContext(context)
        draw(at: .zero)
        UIGraphicsPopContext()

        guard let cgImage = context.makeImage(),
              let dataProvider = cgImage.dataProvider,
              let data = dataProvider.data,
              let pixels = CFDataGetBytePtr(data) else {
            return nil
        }

        let width = cgImage.width
        let height = cgImage.height
        let bytesPerRow = cgImage.bytesPerRow

        var factors: [BlurHashColor] = []
        for componentY in 0 ..< components.1 {
            for componentX in 0 ..< components.0 {
                let normalisation: Float = (componentX == 0 && componentY == 0) ? 1 : 2
                let factor = multiplyBasisFunction(
                    pixels: pixels,
                    width: width,
                    height: height,
                    bytesPerRow: bytesPerRow,
                    bytesPerPixel: cgImage.bitsPerPixel / 8
                ) {
                    normalisation * cos(Float.pi * Float(componentX) * $0 / Float(width)) as Float *
                        cos(Float.pi * Float(componentY) * $1 / Float(height)) as Float
                }
                factors.append(factor)
            }
        }

        guard let directCurrent = factors.first else { return nil }
        let alternatingCurrent = factors.dropFirst()

        var hash = ""

        let sizeFlag = (components.0 - 1) + (components.1 - 1) * 9
        hash += sizeFlag.encode83(length: 1)

        let maximumValue: Float
        if !alternatingCurrent.isEmpty {
            let actualMaximumValue = alternatingCurrent
                .map { max(abs($0.red), abs($0.green), abs($0.blue)) }
                .max()
            guard let actualMaximumValue else { return nil }
            let quantisedMaximumValue = Int(max(0, min(82, floor(actualMaximumValue * 166 - 0.5))))
            maximumValue = Float(quantisedMaximumValue + 1) / 166
            hash += quantisedMaximumValue.encode83(length: 1)
        } else {
            maximumValue = 1
            hash += 0.encode83(length: 1)
        }

        hash += encodeDC(directCurrent).encode83(length: 4)

        for factor in alternatingCurrent {
            hash += encodeAC(factor, maximumValue: maximumValue).encode83(length: 2)
        }

        return hash
    }

    private func multiplyBasisFunction(
        pixels: UnsafePointer<UInt8>,
        width: Int,
        height: Int,
        bytesPerRow: Int,
        bytesPerPixel: Int,
        basisFunction: (Float, Float) -> Float
    ) -> BlurHashColor {
        var red: Float = 0
        var green: Float = 0
        var blue: Float = 0

        let buffer = UnsafeBufferPointer(start: pixels, count: height * bytesPerRow)

        for pixelX in 0 ..< width {
            for pixelY in 0 ..< height {
                let basis = basisFunction(Float(pixelX), Float(pixelY))
                red += basis * BlurHashMath.sRGBToLinear(buffer[bytesPerPixel * pixelX + pixelY * bytesPerRow])
                green += basis * BlurHashMath.sRGBToLinear(buffer[bytesPerPixel * pixelX + 1 + pixelY * bytesPerRow])
                blue += basis * BlurHashMath.sRGBToLinear(buffer[bytesPerPixel * pixelX + 2 + pixelY * bytesPerRow])
            }
        }

        let scale = 1 / Float(width * height)

        return BlurHashColor(red: red * scale, green: green * scale, blue: blue * scale)
    }
}

private func encodeDC(_ value: BlurHashColor) -> Int {
    let roundedR = BlurHashMath.linearToSRGB(value.red)
    let roundedG = BlurHashMath.linearToSRGB(value.green)
    let roundedB = BlurHashMath.linearToSRGB(value.blue)
    return (roundedR << 16) + (roundedG << 8) + roundedB
}

private func encodeAC(_ value: BlurHashColor, maximumValue: Float) -> Int {
    let quantR = Int(max(0, min(18, floor(BlurHashMath.signPow(value.red / maximumValue, 0.5) * 9 + 9.5))))
    let quantG = Int(max(0, min(18, floor(BlurHashMath.signPow(value.green / maximumValue, 0.5) * 9 + 9.5))))
    let quantB = Int(max(0, min(18, floor(BlurHashMath.signPow(value.blue / maximumValue, 0.5) * 9 + 9.5))))

    return quantR * 19 * 19 + quantG * 19 + quantB
}

private extension BinaryInteger {
    func encode83(length: Int) -> String {
        var result = ""
        for position in 1 ... length {
            let digit = (Int(self) / pow(83, length - position)) % 83
            result += BlurHashMath.characters[Int(digit)]
        }
        return result
    }
}

private func pow(_ base: Int, _ exponent: Int) -> Int {
    (0 ..< exponent).reduce(1) { value, _ in value * base }
}
