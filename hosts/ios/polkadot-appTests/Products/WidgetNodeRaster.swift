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
            CustomMessageWidgetView(node: node, resolveImage: nil)
        }
        .frame(width: size.width, height: size.height)

        let renderer = ImageRenderer(content: view)
        renderer.scale = 1

        let image = try #require(renderer.uiImage?.cgImage)

        return buckets(of: image).mapValues(\.count)
    }

    /// Where each colour bucket the node paints lies, drawn in a window of the
    /// app's scene so its image loads run, which `ImageRenderer` skips.
    static func bounds(
        of node: CustomMessageWidgetNode,
        in size: CGSize,
        resolveImage: WidgetImageResolver? = nil
    ) async throws -> [String: CGRect] {
        let view = CustomMessageWidgetView(node: node, resolveImage: resolveImage)
            .frame(width: size.width, height: size.height, alignment: .topLeading)
            .background(Color.black)
        let host = UIHostingController(rootView: view)
        host.safeAreaRegions = []
        let scene = try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        window.rootViewController = host
        window.isHidden = false
        defer { window.isHidden = true }

        try await Task.sleep(for: .milliseconds(300))

        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = try #require(UIGraphicsImageRenderer(size: size, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }.cgImage)

        return buckets(of: image).mapValues { points in
            points.reduce(CGRect.null) { $0.union(CGRect(origin: $1, size: CGSize(width: 1, height: 1))) }
        }
    }

    /// The share of the frame taken by the colour the node paints most of.
    static func largestShare(of node: CustomMessageWidgetNode, in size: CGSize) throws -> Double {
        let counted = try colours(of: node, in: size)
        let total = counted.values.reduce(0, +)

        return Double(counted.values.max() ?? 0) / Double(total)
    }

    private static func buckets(of image: CGImage) -> [String: [CGPoint]] {
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

        var points: [String: [CGPoint]] = [:]
        for index in 0 ..< width * height {
            let pixel = index * 4
            let bucket = "\(pixels[pixel] / 64)-\(pixels[pixel + 1] / 64)-\(pixels[pixel + 2] / 64)"
            points[bucket, default: []].append(CGPoint(x: index % width, y: index / width))
        }

        return points
    }
}
