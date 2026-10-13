import PolkadotUI
import SwiftUI
import Testing
import UIKit

/// Draws a node and reports what reached the pixels, so a test can tell a tree
/// that paints from one that only maps.
@MainActor
enum WidgetNodeRaster {
    /// Colours the node paints over black, each as a coarse `r-g-b` bucket,
    /// counted. Bucketed because anti-aliasing puts an edge across dozens of
    /// neighbouring values that say nothing on their own.
    static func colours(of node: CustomMessageWidgetNode, in size: CGSize) throws -> [String: Int] {
        let view = ZStack {
            Color.black
            CustomMessageWidgetView(node: node)
        }
        .frame(width: size.width, height: size.height)

        let renderer = ImageRenderer(content: view)
        renderer.scale = 1

        let image = try #require(renderer.uiImage?.cgImage)
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let context = CGContext(
            data: &pixels,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
        context?.draw(image, in: CGRect(origin: .zero, size: CGSize(width: width, height: height)))

        var counted: [String: Int] = [:]
        for pixel in stride(from: 0, to: pixels.count, by: 4) {
            let bucket = "\(pixels[pixel] / 64)-\(pixels[pixel + 1] / 64)-\(pixels[pixel + 2] / 64)"
            counted[bucket, default: 0] += 1
        }

        return counted
    }

    /// The share of the frame taken by the colour the node paints most of.
    static func largestShare(of node: CustomMessageWidgetNode, in size: CGSize) throws -> Double {
        let counted = try colours(of: node, in: size)
        let total = counted.values.reduce(0, +)

        return Double(counted.values.max() ?? 0) / Double(total)
    }
}
