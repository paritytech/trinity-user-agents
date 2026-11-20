import Testing
import UIKit
@testable import PolkadotUI

/// Inline bytes come from a product, and a few hundred bytes can declare a picture of any size. Decoding them whole
/// would let a tiny file exhaust memory, so the picture is read at most 1024 pixels on its longer side.
struct InlineWidgetImageTests {
    @Test func decodesAHugeDeclaredPictureWithinTheBound() throws {
        let image = try #require(UIImage(downsampling: png(width: 4_000, height: 400)))

        #expect(image.size.width * image.scale == 1_024)
    }

    @Test func drawsNothingForBytesThatAreNotAPicture() {
        #expect(UIImage(downsampling: Data("not a picture".utf8)) == nil)
    }

    // A card drawn from its kept face before the Pocket has started has nothing to locate images with yet, and its
    // inline images must draw all the same.
    @Test func drawsInlineBytesWithNothingToLocateImagesWith() async {
        #expect(await WidgetImageResolver.inlineOnly(.inline(png(width: 10, height: 10))) != nil)
    }

    private func png(width: CGFloat, height: CGFloat) -> Data {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format).pngData { context in
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        }
    }
}
