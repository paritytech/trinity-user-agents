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

import Foundation

struct BlurHashColor {
    let red: Float
    let green: Float
    let blue: Float
}

enum BlurHashMath {
    static let characters = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~"
        .map { String($0) }
    static let decodeCharacters = Dictionary(
        uniqueKeysWithValues: characters.enumerated().compactMap { index, character in
            character.first.map { ($0, index) }
        }
    )

    static func signPow(_ value: Float, _ exponent: Float) -> Float {
        copysign(pow(abs(value), exponent), value)
    }

    static func linearToSRGB(_ value: Float) -> Int {
        let normalized = max(0, min(1, value))
        if normalized <= 0.0031308 {
            return Int(normalized * 12.92 * 255 + 0.5)
        }
        return Int((1.055 * pow(normalized, 1 / 2.4) - 0.055) * 255 + 0.5)
    }

    static func sRGBToLinear(_ value: some BinaryInteger) -> Float {
        let normalized = Float(Int64(value)) / 255
        if normalized <= 0.04045 {
            return normalized / 12.92
        }
        return pow((normalized + 0.055) / 1.055, 2.4)
    }
}
