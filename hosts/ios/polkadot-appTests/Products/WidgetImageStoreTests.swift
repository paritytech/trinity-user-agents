import CoreGraphics
@testable import PolkadotUI
import SwiftUI
import Testing
import UIKit

@MainActor
struct WidgetImageStoreTests {
    /// A `Materials` base is drawn in one pass, which cannot wait for an image node's task, so a
    /// picture the visible card already loaded reaches the base only through the store.
    @Test func anImageNodeDrawnInOnePassShowsTheStoredPicture() throws {
        // Given
        let source = CustomMessageWidgetNode.ImageSource.archive(path: "red.png")
        let images = WidgetImageStore()
        images.store(Self.solid(.red), for: source)
        let node = CustomMessageWidgetNode(
            content: .image(.init(source: source)),
            modifiers: .init(width: 8, height: 8)
        )
        let renderer = ImageRenderer(
            content: CustomMessageWidgetView(node: node, resolveImage: nil)
                .environment(\.widgetImages, images)
                .frame(width: 8, height: 8)
        )
        renderer.scale = 1

        // When
        let drawn = try #require(renderer.cgImage)

        // Then
        #expect(Self.centre(of: drawn) == [255, 0, 0, 255])
    }
}

private extension WidgetImageStoreTests {
    static func solid(_ colour: UIColor) -> UIImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.preferredRange = .standard
        return UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8), format: format).image { context in
            colour.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }
    }

    static func centre(of image: CGImage) -> [UInt8] {
        var rgba = [UInt8](repeating: 0, count: 4)
        let context = CGContext(
            data: &rgba,
            width: 1,
            height: 1,
            bitsPerComponent: 8,
            bytesPerRow: 4,
            space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
        let centre = CGRect(x: -image.width / 2, y: -image.height / 2, width: image.width, height: image.height)
        context?.draw(image, in: centre)
        return rgba
    }
}
