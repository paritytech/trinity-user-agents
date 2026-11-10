import DesignSystem
import PolkadotUI
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// Draws widgets and reports what reached the pixels, so a test can tell a tree
/// that paints from one that only maps.
@MainActor
enum WidgetNodeRaster {
    /// The bucket of the magenta every node is drawn over, a colour no design
    /// token comes near, so what a node leaves unpainted is told apart from a
    /// dark surface it paints.
    static let backdrop = "3-0-3"

    /// Colours the node paints over the backdrop, each as a coarse `r-g-b`
    /// bucket, counted. Bucketed because anti-aliasing puts an edge across
    /// dozens of neighbouring values that say nothing on their own.
    static func colours(of node: CustomMessageWidgetNode, in size: CGSize) throws -> [String: Int] {
        let view = ZStack {
            backdropColour
            CustomMessageWidgetView(node: node, resolveImage: nil)
        }
        .frame(width: size.width, height: size.height)

        let renderer = ImageRenderer(content: view)
        renderer.scale = 1

        return try buckets(of: #require(renderer.uiImage?.cgImage)).mapValues(\.count)
    }

    private static let backdropColour = Color(.sRGB, red: 1, green: 0, blue: 1)

    private static func pixels(of image: CGImage) -> [UInt8] {
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

        return pixels
    }

    private static func buckets(of image: CGImage) -> [String: (count: Int, bounds: CGRect)] {
        let pixels = pixels(of: image)
        var found: [String: (count: Int, bounds: CGRect)] = [:]
        for index in 0 ..< image.width * image.height {
            let pixel = index * 4
            let bucket = "\(pixels[pixel] / 64)-\(pixels[pixel + 1] / 64)-\(pixels[pixel + 2] / 64)"
            let point = CGRect(x: index % image.width, y: index / image.width, width: 1, height: 1)
            let seen = found[bucket] ?? (0, .null)
            found[bucket] = (seen.count + 1, seen.bounds.union(point))
        }

        return found
    }
}
