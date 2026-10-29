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

    /// Where each colour bucket the node paints lies, drawn in the app's own
    /// window so its image loads run, which `ImageRenderer` skips. Drawn again
    /// until `ready` holds, for up to two seconds, so a picture still loading
    /// is waited for rather than raced.
    static func bounds(
        of node: CustomMessageWidgetNode,
        in size: CGSize,
        resolveImage: WidgetImageResolver? = nil,
        until ready: ([String: CGRect]) -> Bool = { _ in true }
    ) async throws -> [String: CGRect] {
        let view = CustomMessageWidgetView(node: node, resolveImage: resolveImage)
            .frame(width: size.width, height: size.height, alignment: .topLeading)
            .background(backdropColour)
        let window = try window(showing: view, in: size)
        defer { window.isHidden = true }

        var drawn: [String: CGRect] = [:]
        for _ in 0 ..< 20 {
            try await Task.sleep(for: .milliseconds(100))
            drawn = try buckets(of: snapshot(of: window)).mapValues(\.bounds)
            if ready(drawn) { break }
        }
        return drawn
    }

    /// The view drawn on screen in the app's own window under the picked
    /// theme, as RGBA bytes row by row. Offscreen rendering leaves text fields
    /// without the colours a window gives them, so colour checks draw here.
    static func pixels(of view: some View, in size: CGSize, under theme: ThemeSelection) throws -> [UInt8] {
        let window = try window(showing: view, in: size, under: theme)
        defer { window.isHidden = true }

        return try pixels(of: snapshot(of: window))
    }

    private static let backdropColour = Color(.sRGB, red: 1, green: 0, blue: 1)

    private static func window(
        showing view: some View,
        in size: CGSize,
        under theme: ThemeSelection? = nil
    ) throws -> UIWindow {
        let scene = try #require(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = RootWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        if let theme {
            window.traitOverrides.appTheme = theme
        }

        let host = UIHostingController(rootView: view)
        host.safeAreaRegions = []
        host.view.backgroundColor = .black
        window.rootViewController = host
        window.isHidden = false
        return window
    }

    private static func snapshot(of window: UIWindow) throws -> CGImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.preferredRange = .standard
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }

        return try #require(image.cgImage)
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
