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

        let pixels = try pixels(of: #require(renderer.uiImage?.cgImage))

        var counted: [String: Int] = [:]
        for pixel in stride(from: 0, to: pixels.count, by: 4) {
            let bucket = "\(pixels[pixel] / 64)-\(pixels[pixel + 1] / 64)-\(pixels[pixel + 2] / 64)"
            counted[bucket, default: 0] += 1
        }

        return counted
    }

    /// The view drawn on screen in the app's own window under the picked
    /// theme, as RGBA bytes row by row. Offscreen rendering leaves text fields
    /// without the colours a window gives them, so colour checks draw here.
    static func pixels(of view: some View, in size: CGSize, under theme: ThemeSelection) throws -> [UInt8] {
        let scene = try #require(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = RootWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        window.traitOverrides.appTheme = theme

        let host = UIHostingController(rootView: view)
        host.safeAreaRegions = []
        host.view.backgroundColor = .black
        window.rootViewController = host
        window.isHidden = false
        defer { window.isHidden = true }

        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.preferredRange = .standard
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }

        return try pixels(of: #require(image.cgImage))
    }

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

    /// The share of the frame taken by the colour the node paints most of.
    static func largestShare(of node: CustomMessageWidgetNode, in size: CGSize) throws -> Double {
        let counted = try colours(of: node, in: size)
        let total = counted.values.reduce(0, +)

        return Double(counted.values.max() ?? 0) / Double(total)
    }
}
